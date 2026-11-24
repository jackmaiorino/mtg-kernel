"""Which checks a CI run needs: the one table behind the workflow's scoped checks and docs/CI-SCOPES.md.

A ready pull request whose files all map to one scope runs only that scope's checks; files of more than one
scope, or any file no rule names, run the whole pull request set. Docs and website files never decide (website
files alone, with or without docs, are the viewer scope). The full matrix still runs on every push to main and
every tag (and on a dispatch without `scope`, the way release lines are checked) as a monitor, so a scope only
decides what gates a merge (collab HANDOFFS/20261117/COORD.md, "Standing rule, merges (2026-11-21 07:27 ET)").
One exception: a dispatch without inputs at the head of a ready same-repository pull request whose own run
scheduled the whole pull request set (scope pr and Rust-relevant files, `pr-run`) runs only the Windows groups
that set leaves out; `full_matrix=true` runs every group anyway.

Usage:
  ci_scope.py choose          detect-changes step: reads EVENT, SCOPE (the group set chosen before), INPUT_SCOPE,
                              LABELS (JSON list), and NUL-delimited paths on stdin for pull requests;
                              writes checks=<name> to GITHUB_OUTPUT
                              and a summary (with the bit-identity rows for trainer) to GITHUB_STEP_SUMMARY.
  ci_scope.py file-scope      print the scope of the JSON pages from the GitHub pull request files API on stdin.
  ci_scope.py pr-run          the same pages on stdin; print whole-set if that pull request's run scheduled the
                              whole pull request set (scope pr and Rust-relevant files), else partial.
  ci_scope.py relevance       detect-changes step: NUL-delimited pull request paths on stdin and FULL_LABEL;
                              writes python_relevant and rust_relevant to GITHUB_OUTPUT.
  ci_scope.py commands SCOPE  print the shell commands a scope's local run executes, one per line.
  ci_scope.py trainer-filters [--shard N]  print the trainer scope's library test filters (or shard N's).
  ci_scope.py markdown        print docs/CI-SCOPES.md.
"""
from __future__ import annotations

import fnmatch
import json
import os
import sys

# Each rule: (patterns, scope). The first rule naming a file decides it. "shared" files are read by both the
# engine (or agents, search, ladder) and the trainer: a change there is mixed and runs the whole pull request
# set, never a trainer-only check.
TOOLS_DIRS = ["checkpoint_codec_bench", "cube_v1", "native_fixture_bench", "sd1", "search_experiment", "stage4a", "t1_diagnosis"]
TRAINER_ROOTS = ["expanded_deck_training_v1", "native_expanded_training_run_v1", "native_policy_train_step_v1"]
PATH_RULES: list[tuple[tuple[str, ...], str]] = [
    (("data/*",), "pr"),
    (("docs/*", "*.md"), "docs"),
    ((".github/workflows/ci.yml", ".github/actions/*", "tools/ci/*"), "pr"),
    # w5-tools.yml also watches a few files python/tests reads (tools/spy_reset, ex1, two python/tools
    # modules); those run the Python set, and w5-tools.yml still runs on them by its own paths.
    (("tools/w5/*", ".github/workflows/w5-tools.yml"), "w5-tools"),
    ((".github/workflows/*",), "workflow"),
    # The website (viewer.yml builds, tests and publishes it). No crate source, Rust test, Python test or other
    # workflow reads a file under viewer/ (tools/ci/tests/test_ci_scope.py pins the files that name it);
    # python/tools/viewer_* and python/tests/test_viewer_* are Python files and keep the Python set. Neutral
    # like docs: with other files, those files decide alone.
    (("viewer/*",), "viewer"),
    # Tool directories whose tests all live beside them (no python/tests or python/tools module reads them);
    # every other tools/** directory has tests in python/tests and runs the Python set.
    (tuple(f"tools/{name}/*" for name in TOOLS_DIRS), "tools"),
    (("tools/*",), "python"),
    # Shared: observation encoding, action windows, mulligans, sampling, the policy network, and every part
    # of the trainer that code outside it reads (the module root's inference loaders, the train step's 41
    # outside users, the submodules listed). tools/ci/tests/test_ci_scope.py fails if a trainer file below
    # becomes readable from outside.
    (("mtg-kernel/src/*mulligan*", "mtg-kernel/src/*action_windows*", "mtg-kernel/src/policy_observation_*",
      "mtg-kernel/src/native_flat_tensorizer_*", "mtg-kernel/src/native_policy_value_net_v1*",
      "mtg-kernel/src/fast_sampler.rs", "mtg-kernel/src/unclamped_softmax_sampler_v1.rs",
      "mtg-kernel/src/sideboard_play_policy_v1*", "mtg-kernel/src/rl_session.rs", "mtg-kernel/src/rl_session/*",
      # The whole of expanded_deck_training_v1: its root's inference loaders (human matches, agents, the
      # Spellbench scorer) reach into its submodules through grouped imports and re-exports.
      "mtg-kernel/src/expanded_deck_training_v1.rs", "mtg-kernel/src/expanded_deck_training_v1/*",
      "mtg-kernel/src/native_policy_train_step_v1.rs", "mtg-kernel/src/native_policy_train_step_v1/*"), "shared"),
    (("mtg-kernel/src/native_expanded_training_run_v1.rs", "mtg-kernel/src/native_expanded_training_run_v1/*",
      "mtg-kernel/src/bin/native_expanded_training_run_v1.rs"), "trainer"),
    # A Limited binary with its own tests runs the whole set: the rules-limited jobs only build the binaries.
    (("mtg-kernel/src/bin/kernel_limited_training_v1.rs",), "pr"),
    (("mtg-kernel/src/bin/kernel_limited_*",), "rules-limited"),
    # Python files that change which Rust tests run stay with the whole set.
    (("python/tools/run_rust_workspace_tests.py", "python/tools/run_rust_lib_filters.py",
      "python/tools/rust_case_seconds.json", "python/tools/rust_case_timeout_seconds.json",
      "python/tools/rust_limited_only_known_failures.json", "python/tools/run_rust_integration_tests.py",
      "python/tools/rust_integration_module_seconds.json", "python/tools/check_rust_integration_inventory.py",
      "python/tools/check_rust_integration_parts.py",
      "python/tools/limited_*_session_v*.py"), "pr"),
    (("python/tests/*", "python/tools/*"), "python"),
]
NEUTRAL = {"docs", "viewer"}
# What each scope runs. `jobs` are ci.yml job ids; the whole-set jobs gate themselves by scope as before.
SCOPES: dict[str, dict] = {
    "pr": {"jobs": ["the whole pull request set"],
           "summary": "every job of the pull request set (today's behavior)"},
    "trainer": {"jobs": ["rust-quality", "trainer-tests"],
                "summary": "formatting and Clippy, plus the library tests of the trainer and learner modules and "
                           "of what feeds a training update (TRAINER_FILTERS), Linux"},
    "tools": {"jobs": ["python-tools-tests"],
              "summary": "the unit suites beside the tool directories in TOOLS_DIRS (no Rust binary), Linux"},
    "w5-tools": {"jobs": [],
                 "summary": "nothing in ci.yml: w5-tools.yml runs its Linux (Python 3.11) and Windows unit jobs and "
                            "the cgroup acceptance on these paths"},
    "python": {"jobs": ["python-env-build", "python-tests"],
               "summary": "the Python test binaries and shards, Linux and Windows"},
    "rules-limited": {"jobs": ["rust-quality", "limited-build", "rust-limited-groups"],
                      "summary": "formatting and Clippy, the shared Limited build and every Limited group, Linux "
                                 "(only Limited process binaries map here; Limited library code is engine code)"},
    "workflow": {"jobs": [], "summary": "nothing in ci.yml: the changed workflow runs itself on its own paths"},
    "viewer": {"jobs": [], "summary": "no Rust or Python job (detect-changes only): viewer.yml builds and tests the "
                                      "website on these paths (and publishes it from main)"},
    "docs": {"jobs": [], "summary": "nothing"},
}
SCOPED = [name for name in SCOPES if name != "pr"]
DISPATCH_SCOPES = ["trainer", "tools", "python", "rules-limited"]
# Library test filters (libtest substring filters) of the trainer scope: the trainer and learner modules and
# everything that feeds a training update (GAE and the policy train step, both backward paths and their
# differential tests, the tensorizers and replay caches, the keep-lean and pass-lean offsets, which live in
# expanded_deck_training_v1, the checkpoint container and receipt code).
TRAINER_FILTERS = [
    "expanded_deck_training_v1::", "native_policy_train_step_v1::", "native_expanded_training_run_v1::",
    "native_policy_value_net_v1::", "training_math_v1::", "native_flat_tensorizer_v", "inference_cache_v1::",
    "native_train_state_payload_v1::", "native_trainer_schedule_v", "native_trainer_v1::", "native_training_",
    "native_checkpoint_", "native_baseline_checkpoint_chain_v4::", "fast_sampler::", "unclamped_softmax_sampler_v1::",
    "s_transformer_v1::",
    # Update targets and sampling the learners call: the EX1 target, the W5 exact target and the line-B teacher.
    "ex1_v1::", "w5_xact_v1::", "line_b_teacher_target_v1::",
    # The scalar and vector activation math training_math_v1 calls.
    "deterministic_math_v1::", "vector_tanh_v1::",
    # The rest of the update path the train step, training math and the trainer reference: the retention anchor
    # (KL gradient), the training baseline, the CUDA training bridge, the exact GPU learner, the trajectory records
    # the update consumes and the learner's model snapshot.
    "native_policy_anchor_v1::", "native_policy_baseline_state_v4::", "native_full_episode_trajectory_v",
    "private_physical_trajectory_", "common_model_snapshot_v1::",
    # Compiled only with the CUDA features (third shard, built with them, as the full set's host-safe CUDA step):
    # the CUDA training bridge, the exact GPU learner and the trainer's CUDA-gated tests.
    "experimental_burn_net8_packed_v1::", "gpu_exact_v1::", "expanded_deck_training_v1::public_features::",
    "expanded_deck_training_v1::stack_features::",
    "expanded_deck_training_v1::tests::ordinary_trainer_v4_update_rejects_fixed_partition_backward_on_cuda",
]
# The trainer tests run as two jobs, each building the library and running its filters. Balanced from the
# filter times of hosted run 44906865422 (seconds): expanded_deck_training_v1 408, native_trainer_v1 306,
# native_training_ 187, native_policy_train_step_v1 74, native_expanded_training_run_v1 49,
# native_checkpoint_ 11, the rest under 2 each; about 531 s and 505 s. s_transformer_v1 (the S learner's
# update, added after that run) joins the second.
TRAINER_SHARDS = [
    ["expanded_deck_training_v1::", "native_policy_train_step_v1::", "native_expanded_training_run_v1::"],
    ["native_policy_value_net_v1::", "training_math_v1::", "native_flat_tensorizer_v", "inference_cache_v1::",
     "native_train_state_payload_v1::", "native_trainer_schedule_v", "native_trainer_v1::", "native_training_",
     "native_checkpoint_", "native_baseline_checkpoint_chain_v4::", "fast_sampler::", "unclamped_softmax_sampler_v1::",
     "s_transformer_v1::", "ex1_v1::", "w5_xact_v1::", "line_b_teacher_target_v1::", "deterministic_math_v1::",
     "vector_tanh_v1::"],
]
# Filters added after the timing run go to the first shard (its measured share was the larger only by 26 s).
TRAINER_SHARDS[0] += ["native_policy_anchor_v1::", "native_policy_baseline_state_v4::",
                      "native_full_episode_trajectory_v", "private_physical_trajectory_", "common_model_snapshot_v1::"]
TRAINER_SHARDS.append(["experimental_burn_net8_packed_v1::", "gpu_exact_v1::",
                       "expanded_deck_training_v1::public_features::", "expanded_deck_training_v1::stack_features::",
                       "expanded_deck_training_v1::tests::ordinary_trainer_v4_update_rejects_fixed_partition_backward_on_cuda"])
# Cargo features each shard builds with ("" is the default build). The CUDA shard uses the full set's host-safe
# CUDA test configuration (no device on hosted runners; tests that need one are #[ignore]d).
TRAINER_SHARD_FEATURES = ["", "", "experimental-burn-net8-packed-cuda-v1,gpu-exact-v1"]
# Modules the train step, training math, the trainer and the trainer-scoped run module reference that the filters
# leave out, each with its reason. The trainer-tests job fails on any referenced module that is neither filtered
# nor listed here, so a new backend is examined before it can skip its tests.
TRAINING_REFERENCE_EXCLUSIONS = {
    "async_flat_scored_rollout_v1": "self-play rollout collection, not update arithmetic",
    "async_flat_scored_rollout_v2": "self-play rollout collection, not update arithmetic",
    "async_rollout": "self-play rollout collection, not update arithmetic",
    "async_rollout_v2": "self-play rollout collection, not update arithmetic",
    "card_def": "engine card database hash; engine changes run the whole set",
    "durable_publication_v1": "file publication of artifacts",
    "engine_identity_v1": "build provenance checked at run start, not update arithmetic",
    "flat_policy_v2": "action-surface contract: decision views and buffers, not update arithmetic",
    "flat_policy_v3": "action-surface contract: decision encoder and views, not update arithmetic",
    "flat_policy_v4": "action-surface contract: decision encoder and views, not update arithmetic",
    "kernel_native_search_opponent_v1": "opponent policy for collection",
    "native_ladder_opponent_v1": "opponent policy for collection",
    "native_population_opponent_v1": "opponent policy for collection",
    "policy_observation_v6": "observation contract (shared: a change there runs the whole set)",
    "regret_census_v1": "diagnostic census, not part of an update",
    "rl": "engine types; engine changes run the whole set",
    "rl_session": "game session (shared: a change there runs the whole set)",
    "runtime_decks": "deck data for collection",
    "sideboard": "engine sideboarding; engine changes run the whole set",
    "sideboard_play_policy_v1": "sideboard policy (shared: a change there runs the whole set)",
    "state": "engine state and its seed stream; engine changes run the whole set",
    "test_temp_dir_v1": "test utility",
}
TRAINING_REFERENCE_ROOTS = ["mtg-kernel/src/native_policy_train_step_v1.rs", "mtg-kernel/src/native_policy_train_step_v1",
                            "mtg-kernel/src/training_math_v1.rs", "mtg-kernel/src/native_trainer_v1.rs",
                            "mtg-kernel/src/native_trainer_v1", "mtg-kernel/src/native_expanded_training_run_v1.rs",
                            "mtg-kernel/src/native_expanded_training_run_v1"]
def filtered(module: str) -> bool:
    """Whether the trainer filters select a crate module's tests: a `name::` filter selects exactly that module,
    a prefix filter (no trailing ::) every module starting with it."""
    return any(module == f[:-2] if f.endswith("::") else module.startswith(f) for f in TRAINER_FILTERS)


def crate_references(file, depth: int) -> set[str]:
    """Crate-root modules a Rust file names: `crate::x`, grouped `crate::{x, y::z}`, and `super::` chains that
    climb to the crate root from the file's module depth plus any enclosing inline modules."""
    import rust_tokens  # the tokenizer (strings, comments and raw strings handled)
    toks = rust_tokens.tokens(file.read_text(encoding="utf-8", errors="replace"))
    out, stack, i = set(), [], 0

    def group(j: int) -> int:
        """toks[j] == '{': add the first name of each top-level item; return the index after the group."""
        level, first = 0, True
        while True:
            t = toks[j]
            if t == "{":
                level += 1
                first = first or level == 1
            elif t == "}":
                level -= 1
                if level == 0:
                    return j + 1
            elif t == "," and level == 1:
                first = True
            elif level == 1 and first and re.fullmatch(r"[a-z_][a-z_0-9]*", t):
                if t not in ("self", "super", "crate"):
                    out.add(t)
                first = False
            elif level == 1 and first and t == "*":
                out.add("*")
                first = False
            j += 1

    import re
    while i < len(toks):
        t = toks[i]
        if t == "mod" and i + 2 < len(toks) and toks[i + 2] == "{":
            stack.append("mod")
            i += 3
            continue
        if t == "{":
            stack.append("{")
        elif t == "}":
            if stack:
                stack.pop()
        elif t in ("crate", "super") and toks[i + 1:i + 3] == [":", ":"]:
            j, climbs = i, 0
            if t == "super":
                while toks[j] == "super" and toks[j + 1:j + 3] == [":", ":"]:
                    climbs += 1
                    j += 3
                if climbs < depth + stack.count("mod"):
                    i = j
                    continue
            else:
                j += 3
            if toks[j] == "{":
                i = group(j)
                continue
            if re.fullmatch(r"[a-z_][a-z_0-9]*", toks[j]) or toks[j] == "*":
                out.add(toks[j])
            i = j + 1
            continue
        i += 1
    return out


def training_reference_gaps(root=None) -> tuple[list[str], list[str]]:
    """(referenced modules neither filtered nor excluded, exclusions no longer referenced) for the files in
    TRAINING_REFERENCE_ROOTS. The trainer scope runs this, so a qualifying change cannot add an unexamined backend."""
    from pathlib import Path
    base = Path(root) if root else Path(__file__).resolve().parents[2]
    src = base / "mtg-kernel" / "src"
    referenced = set()
    for name in TRAINING_REFERENCE_ROOTS:
        path = base / name
        for file in ([path] if path.is_file() else sorted(path.rglob("*.rs"))):
            parts = file.relative_to(src).with_suffix("").parts
            depth = len(parts) - (1 if parts[-1] == "mod" else 0)
            referenced |= crate_references(file, depth)
    gaps = sorted(m for m in referenced if m not in TRAINING_REFERENCE_EXCLUSIONS and not filtered(m))
    return gaps, sorted(set(TRAINING_REFERENCE_EXCLUSIONS) - referenced)


# Trainer changes also need these rows, which run on a node, never in CI.
BIT_IDENTITY_ROWS = [
    "the 10-game main-chain recipe", "the 60-game recipe", "the pilot recipe with mulligans",
    "the pilot recipe with mulligans and priority windows",
]
BIT_IDENTITY_BUILDS = "on the znver5 build and on the portable build, and on native arm64 when Mac arms use that build"
TRAINER_RULE = ("A trainer pull request without those rows linked is not mergeable under the merge rule, "
                "whatever CI says.")
RULES_NOTE = ("Engine rules and card changes that alter play are never bit-identical by scope: they run the rules "
              "groups (the whole set, or rules-limited), and, if a running lineage's trainer would pick them up, "
              "need a declaration.")
# Shell commands of each scope's local run (tools/ci/run_scope_local.py), from the repository root. The tools
# commands are the same lines the python-tests job runs (a test keeps them equal).
TOOLS_COMMANDS = [f"uv run --no-sync python -m unittest discover -v -s tools/{name}/{sub} -p 'test_*.py'"
                  for name, sub in (("search_experiment", "tests"), ("stage4a", "tests"), ("t1_diagnosis", ""),
                                    ("sd1", "tests"), ("native_fixture_bench", "tests"),
                                    ("checkpoint_codec_bench", "tests"), ("cube_v1", "tests"))]
TOOLS_COMMANDS = [c.replace("/ -p", " -p") for c in TOOLS_COMMANDS]


# Which job families a pull request's paths make relevant (the only copy: detect-changes' "Classify changed
# paths" step and the dispatch step's `pr-run` both call relevance()). Python jobs care about python sources, tools
# and tests, the data artifacts they validate, workflow changes and the pinned interpreter and lock config.
# Top-level tools/** is Python only; python/tests imports several of its packages, and the lease adapter's tests
# run in the Python job's lease guard step.
PYTHON_RELEVANT = ["python/*", "data/*", ".github/*", "uv.lock", "pyproject.toml", ".python-version", "tools/*"]
# Rust jobs run unless a file is provably outside the crate's input surface (first match decides; a path no rule
# names is relevant). python/mtg_kernel_rl/** stays relevant (embedded authorities: features.py and twins); data/**
# stays relevant (include_str goldens). Native CI runners are also relevant because they determine which tests
# execute. Other non-embedded python, top-level tools/** (Python and PowerShell scripts no crate source or test
# reads), docs and markdown are irrelevant. build.rs still hashes every tracked file into its provenance digest; no
# test pins that value. viewer/** (the website, the viewer scope, neutral like docs) is irrelevant too: no crate
# source, Rust or Python test, or other workflow reads a file there (tools/ci/tests/test_ci_scope.py pins the
# files that name viewer/), and viewer.yml builds and tests it on those paths.
RUST_RELEVANT_RULES: list[tuple[tuple[str, ...], bool]] = [
    (("python/tools/run_rust_workspace_tests.py", "python/tools/run_rust_lib_filters.py",
      "python/tools/rust_case_seconds.json", "python/tools/rust_case_timeout_seconds.json",
      "python/tools/rust_limited_only_known_failures.json", "python/tools/run_rust_integration_tests.py",
      "python/tools/rust_integration_module_seconds.json", "python/tools/check_rust_integration_inventory.py",
      "python/tools/check_rust_integration_parts.py", "python/tools/limited_muldrotha_session_v12.py",
      "python/tools/limited_tinybones_session_v13.py", "python/tools/limited_liliana_session_v14.py",
      "python/tools/limited_chandra_session_v15.py", "python/tools/limited_kaito_session_v16.py"), True),
    (("python/tests/*", "python/tools/*", "tools/*", "docs/*", "*.md", "viewer/*"), False),
]


def relevance(paths: list[str], full_label: bool = False) -> tuple[bool, bool]:
    """(python_relevant, rust_relevant) of a pull request's paths. ci:full asks for the whole pull request set:
    every Rust and Python job, whatever the paths. No paths (a diff that came back empty) fails closed: both.
    Patterns match like shell case patterns (`*` crosses `/`)."""
    if full_label or not paths:
        return True, True
    python = rust = False
    for path in paths:
        python = python or any(fnmatch.fnmatchcase(path, pattern) for pattern in PYTHON_RELEVANT)
        rule = next((value for patterns, value in RUST_RELEVANT_RULES
                     if any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns)), True)
        rust = rust or rule
    return python, rust


def pr_run_is_whole_set(paths: list[str]) -> bool:
    """Whether a ready, unlabelled pull request's run with these files scheduled the whole pull request set:
    checks all (scope pr) and the Rust jobs (Rust-relevant files). Only then may a dispatch at its head run just
    the Windows complement: otherwise the pull request run left out Linux Rust, lint and the pull request's Windows
    groups, which the complement cannot recover. (A ci:full run did schedule them; the dispatch step does not see
    labels, so such a dispatch runs the full set, which only costs time.)"""
    return scope_of_files(paths) == "pr" and relevance(paths)[1]


def scope_of(path: str) -> str | None:
    for patterns, scope in PATH_RULES:
        if any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns):
            return scope
    return None


def scope_of_files(paths: list[str]) -> str:
    """One scope for a change: docs and website files never decide; one remaining scope wins; none means viewer
    if a website file changed, else docs; anything else (mixed, shared or unmapped) is the whole pull request set.
    So website files plus other files classify exactly as the other files alone."""
    found, neutral = set(), set()
    for path in paths:
        scope = scope_of(path)
        (neutral if scope in NEUTRAL else found).add(scope)
    if not found:
        return "viewer" if "viewer" in neutral else "docs"
    if len(found) == 1:
        (only,) = found
        if only in SCOPES and only != "pr":
            return only
    return "pr"


def pull_request_paths(pages: object) -> list[str]:
    # GitHub's REST OpenAPI diff-entry.status enum.
    statuses = ("added", "removed", "modified", "renamed", "copied", "changed", "unchanged")
    if not isinstance(pages, list) or not pages or any(not isinstance(page, list) for page in pages):
        raise ValueError("expected an array of file pages")
    paths = []
    for page in pages:
        for entry in page:
            if not isinstance(entry, dict) or entry.get("status") not in statuses:
                raise ValueError("expected a changed-file object with a recognized status")
            names = [entry.get("filename")]
            if entry["status"] == "renamed" or "previous_filename" in entry:
                names.append(entry.get("previous_filename"))
            if any(not isinstance(name, str) or not name or "\0" in name for name in names):
                raise ValueError("expected a nonempty filename for each changed path")
            paths.extend(names)
    return paths


def choose(event: str, group_set: str, input_scope: str, labels: list[str], files: list[str]) -> str:
    """checks=all keeps the run's group set as chosen before (full, pr, draft, windows-*); a scope name runs
    only that scope's jobs."""
    if event == "workflow_dispatch":
        return input_scope if input_scope in DISPATCH_SCOPES else "all"
    if event != "pull_request" or group_set != "pr":
        return "all"
    if "ci:full" in labels or not files:
        # An empty file list (a diff that came back empty) fails closed, like relevance().
        return "all"
    # Labels never narrow a run: paths decide, ci:full widens.
    scope = scope_of_files(files)
    return "all" if scope == "pr" else scope


def trainer_record() -> str:
    rows = "; ".join(BIT_IDENTITY_ROWS)
    return (f"Trainer scope: CI runs formatting, Clippy and the trainer library tests only (a local run covers "
            f"formatting, workspace Clippy and the library tests; the feature Clippy runs stay in the hosted "
            f"rust-quality job). The bit-identity rows "
            f"go with every trainer change and run on a node, not in CI: {rows}, {BIT_IDENTITY_BUILDS}. "
            f"{TRAINER_RULE}")


def markdown() -> str:
    out = ["# CI scopes", "",
           "Generated by `python tools/ci/ci_scope.py markdown` from `tools/ci/ci_scope.py`; "
           "`tools/ci/tests/test_ci_scope.py` fails if this file and the table differ.", "",
           "A ready pull request whose files all map to one scope runs only that scope's checks. Files of more than "
           "one scope, a shared file, or a file no rule names run the whole pull request set. Docs and website files "
           "never decide: website files alone (with or without docs) are `viewer`, and with other files those "
           "files decide alone. Main pushes, tags and dispatches without `scope` run the full matrix as a monitor "
           "(one exception: a dispatch without inputs at the head of a ready same-repository pull request whose own "
           "run scheduled the whole pull request set, scope pr with Rust-relevant files, runs only the Windows groups "
           "that set leaves out; `full_matrix=true` runs every group anyway): pushes to a line share one concurrency "
           "group (one running, one waiting, a newer merge replaces the "
           "waiting one), and a red monitor's report names every merge it covers. `ci:full` on a pull request "
           "forces the whole pull request set (every Rust and Python job, whatever the paths) from the next push, "
           "reopen or ready-for-review event (adding the label starts no run); labels never narrow a run. "
           "`gh workflow run ci.yml --ref REF -f scope=SCOPE` runs one scope on any ref.", "",
           "## Path rules (first match decides)", "", "| paths | scope |", "|---|---|"]
    for patterns, scope in PATH_RULES:
        out.append(f"| {', '.join('`' + p + '`' for p in patterns)} | {scope} |")
    out += ["| anything else | pr |", "", "`shared`: read by both the engine (or agents, search, ladder) and the "
            "trainer, so a change there is mixed and runs the whole pull request set.", "",
            "## Scope to checks", "", "| scope | ci.yml jobs | what runs |", "|---|---|---|"]
    for name, spec in SCOPES.items():
        out.append(f"| {name} | {', '.join(spec['jobs']) or 'none'} | {spec['summary']} |")
    out += ["", "## Trainer scope", "", "Library test filters: " + ", ".join(f"`{f}`" for f in TRAINER_FILTERS) + ".",
            "", "They run as " + str(len(TRAINER_SHARDS)) + " jobs: " + "; ".join(
                f"shard {i}: " + ", ".join(f"`{f}`" for f in shard) for i, shard in enumerate(TRAINER_SHARDS)) + ".",
            "", trainer_record(), "", "## Rules and cards", "", RULES_NOTE, ""]
    return "\n".join(out)


def main(argv: list[str]) -> int:
    command = argv[1] if len(argv) > 1 else ""
    if command == "markdown":
        sys.stdout.write(markdown())
        return 0
    if command == "check-references":
        gaps, stale = training_reference_gaps()
        for module in gaps:
            print(f"{module}: referenced by the train step or training math; add it to TRAINER_FILTERS or to "
                  "TRAINING_REFERENCE_EXCLUSIONS with a reason")
        for module in stale:
            print(f"{module}: listed in TRAINING_REFERENCE_EXCLUSIONS but no longer referenced; remove it")
        if not gaps and not stale:
            print("references: every module the train step and training math reference is filtered or excluded")
        return 1 if gaps or stale else 0
    if command == "trainer-filters":
        # With --shard N, one shard's filters (the trainer-tests matrix); otherwise all of them.
        print(" ".join(TRAINER_SHARDS[int(argv[3])] if argv[2:3] == ["--shard"] else TRAINER_FILTERS))
        return 0
    if command == "trainer-features":
        print(TRAINER_SHARD_FEATURES[int(argv[3])])
        return 0
    if command == "commands":
        scope = argv[2]
        lines = {"tools": TOOLS_COMMANDS,
                 "trainer": ["cargo fmt --all -- --check",
                             "cargo clippy --release --locked --workspace --all-targets -- -D warnings",
                             *[f'python python/tools/run_rust_lib_filters.py --features "{features}" ' + " ".join(shard)
                               for shard, features in zip(TRAINER_SHARDS, TRAINER_SHARD_FEATURES)]]}.get(scope)
        if lines is None:
            print(f"no local command list for scope {scope}", file=sys.stderr)
            return 2
        print("\n".join(lines))
        return 0
    if command in ("file-scope", "pr-run", "relevance") and len(argv) != 2:
        print(f"{command} takes no arguments", file=sys.stderr)
        return 2
    if command in ("file-scope", "pr-run"):
        # pr-run prints whole-set only as its last act after every check passed; the dispatch step treats any
        # other output, an error or no output as "run the full set".
        try:
            paths = pull_request_paths(json.load(sys.stdin.buffer))
        except ValueError as exc:
            print(f"Invalid pull request files response: {exc}", file=sys.stderr)
            return 2
        if command == "pr-run":
            python, rust = relevance(paths)
            print(f"scope {scope_of_files(paths)}, python_relevant={str(python).lower()}, "
                  f"rust_relevant={str(rust).lower()}", file=sys.stderr)
            print("whole-set" if pr_run_is_whole_set(paths) else "partial")
        else:
            print(scope_of_files(paths))
        return 0
    if command == "relevance":
        # Both outputs or neither: every Rust job reads a missing rust_relevant as "not relevant" and would skip
        # while this step stayed green. So any error exits non-zero (the detect job goes red), and the two lines
        # are written in ONE write, as the last act, after the whole verdict is computed.
        output = os.environ.get("GITHUB_OUTPUT")
        if not output:
            print("GITHUB_OUTPUT is not set: refusing to classify without a place for both outputs", file=sys.stderr)
            return 2
        raw = sys.stdin.buffer.read().split(b"\0")
        files = [path.decode("utf-8", errors="surrogateescape") for path in raw if path]
        print("changed files:")
        for path in files:
            print(f"  {path!r}")
        full_label = os.environ.get("FULL_LABEL") == "true"
        python, rust = relevance(files, full_label)
        if full_label:
            print("ci:full: every Rust and Python job runs")
        elif not files:
            print("no changed files listed: every Rust and Python job runs (fail closed)")
        text = f"python_relevant={str(python).lower()}\nrust_relevant={str(rust).lower()}\n"
        print("verdict: " + text.replace("\n", " ").strip())
        with open(output, "a", encoding="utf-8") as out:
            out.write(text)
        return 0
    if command == "choose":
        labels = json.loads(os.environ.get("LABELS") or "[]")
        event = os.environ.get("EVENT", "")
        files = []
        if event == "pull_request":
            paths = sys.stdin.buffer.read().split(b"\0")
            files = [path.decode("utf-8", errors="surrogateescape") for path in paths if path]
        checks = choose(event, os.environ.get("SCOPE", ""), os.environ.get("INPUT_SCOPE", ""), labels, files)
        lines = [f"checks: {checks}" + ("" if checks == "all" else f" ({SCOPES[checks]['summary']})")]
        if checks == "trainer":
            lines.append(trainer_record())
        print("\n".join(lines))
        for name, text in (("GITHUB_OUTPUT", f"checks={checks}\n"), ("GITHUB_STEP_SUMMARY", "\n\n".join(lines) + "\n")):
            path = os.environ.get(name)
            if path:
                with open(path, "a", encoding="utf-8") as out:
                    out.write(text)
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
