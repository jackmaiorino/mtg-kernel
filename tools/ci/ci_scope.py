"""Which checks a CI run needs on the release line: the one table behind its scoped checks and docs/CI-SCOPES.md.

The release line has one narrow scope, release-trainer (the lead's ruling of 2026-11-21): a change confined to
the trainer and learner modules plus a named list of shared modules runs formatting, Clippy and the library
tests of every one of those modules in full. Any other file (engine rules, cards, data, build files, workflow)
runs the whole pull request set, and so does any pull request that contains a merge commit (a merge between
this line and main). The full matrix runs after every merge into the line as a monitor.

Usage:
  ci_scope.py choose          detect-changes step: reads EVENT, SCOPE, INPUT_SCOPE, LABELS (JSON list),
                              CHANGED_FILES (newline list), MERGE_COMMITS (count); writes checks=<name>.
  ci_scope.py file-scope      print the scope of the file list on stdin.
  ci_scope.py commands SCOPE  print the shell commands of a scope's local run.
  ci_scope.py trainer-filters print the library test filters of release-trainer, space separated.
  ci_scope.py markdown        print docs/CI-SCOPES.md.
"""
from __future__ import annotations

import fnmatch
import json
import os
import sys

# Shared modules that qualify for release-trainer, tested in full: the policy value net, the tensorizers, the
# samplers, the sideboard policy, rl_session, and the trainer's observation code (public_features, and
# stack_features, which encodes the stack for the same observation the policy reads).
SHARED_QUALIFYING = [
    "mtg-kernel/src/native_policy_value_net_v1.rs", "mtg-kernel/src/native_policy_value_net_v1/*",
    "mtg-kernel/src/native_flat_tensorizer_*",
    "mtg-kernel/src/fast_sampler.rs", "mtg-kernel/src/unclamped_softmax_sampler_v1.rs",
    "mtg-kernel/src/native_opponent_sampler_v1.rs",
    "mtg-kernel/src/sideboard_play_policy_v1.rs", "mtg-kernel/src/sideboard_play_policy_v1/*",
    "mtg-kernel/src/rl_session.rs", "mtg-kernel/src/rl_session/*",
]
TRAINER_MODULES = [
    "mtg-kernel/src/expanded_deck_training_v1.rs", "mtg-kernel/src/expanded_deck_training_v1/*",
    "mtg-kernel/src/native_policy_train_step_v1.rs", "mtg-kernel/src/native_policy_train_step_v1/*",
    "mtg-kernel/src/native_expanded_training_run_v1.rs", "mtg-kernel/src/native_expanded_training_run_v1/*",
    "mtg-kernel/src/bin/native_expanded_training_run_v1.rs", "mtg-kernel/src/bin/expanded_deck_training_v1.rs",
    "mtg-kernel/src/bin/checkpoint_container_v1.rs",
]
PATH_RULES: list[tuple[tuple[str, ...], str]] = [
    (("data/*",), "pr"),
    (("*.md",), "docs"),
    (tuple(TRAINER_MODULES + SHARED_QUALIFYING), "release-trainer"),
]
NEUTRAL = {"docs"}
SCOPES: dict[str, dict] = {
    "pr": {"jobs": ["the whole pull request set"], "summary": "every job of the pull request set"},
    "release-trainer": {"jobs": ["rust-quality", "trainer-tests"],
                        "summary": "formatting and Clippy, plus the library tests of the trainer modules and of "
                                   "every qualifying shared module in full (TRAINER_FILTERS), Linux"},
    "docs": {"jobs": [], "summary": "nothing"},
}
DISPATCH_SCOPES = ["release-trainer"]
# Library test filters (libtest substring filters): the trainer and learner modules, everything that feeds a
# training update (GAE, the train step, both backward paths and their differential tests, tensorizers, replay
# caches, the keep-lean and pass-lean offsets in expanded_deck_training_v1, checkpoint container and receipt
# code), and each qualifying shared module in full.
TRAINER_FILTERS = [
    "expanded_deck_training_v1::", "native_policy_train_step_v1::", "native_expanded_training_run_v1::",
    "native_policy_value_net_v1::", "training_math_v1::", "native_flat_tensorizer_v", "inference_cache_v1::",
    "native_train_state_payload_v1::", "native_trainer_schedule_v", "native_trainer_v1::", "native_training_",
    "native_checkpoint_", "native_baseline_checkpoint_chain_v4::", "fast_sampler::", "unclamped_softmax_sampler_v1::",
    "native_opponent_sampler_v1::", "sideboard_play_policy_v1::", "rl_session::",
]
# Some tests in the modules the filters select compile only under other features. Each extra feature set runs the
# filters whose modules have tests gated on a feature that set owns (FEATURE_FILTERS): the set naming the feature,
# or else the first set that enables it through Cargo implications. The default set runs every filter.
# tools/ci/tests/test_ci_scope.py scans every module the filters select and fails if a gate is not covered.
FEATURE_SETS = ["", "experimental-burn-net8-packed-cuda-v1,gpu-exact-fault-injection-v1", "limited-fdn-fixtures",
                "standard-magezero-fixtures", "eternal-fixtures",
                "viewer-recording-v1,flat-action-diagnostic,tensorize-cost-profile-v1,s4a-profile-v1,native-training-store-v2-production"]
FEATURE_FILTERS = {
    "experimental-burn-net8-packed-cuda-v1,gpu-exact-fault-injection-v1": [
        "expanded_deck_training_v1::", "native_policy_train_step_v1::", "native_expanded_training_run_v1::",
        "native_policy_value_net_v1::", "native_trainer_v1::", "native_training_", "native_checkpoint_",
        "sideboard_play_policy_v1::", "rl_session::"],
    "limited-fdn-fixtures": ["native_policy_value_net_v1::", "native_flat_tensorizer_v", "native_training_",
                             "sideboard_play_policy_v1::", "rl_session::"],
    "standard-magezero-fixtures": ["native_flat_tensorizer_v", "native_training_", "rl_session::"],
    "eternal-fixtures": ["rl_session::"],
    "viewer-recording-v1,flat-action-diagnostic,tensorize-cost-profile-v1,s4a-profile-v1,native-training-store-v2-production": [
        "expanded_deck_training_v1::", "native_flat_tensorizer_v", "inference_cache_v1::", "native_trainer_v1::",
        "native_training_", "native_checkpoint_", "sideboard_play_policy_v1::", "rl_session::"],
}

def filters_for(features: str) -> list[str]:
    return TRAINER_FILTERS if not features else FEATURE_FILTERS[features]


BIT_IDENTITY_ROWS = [
    "the production 10-game recipe", "the production 60-game recipe", "the pilot recipe with mulligans",
    "the pilot recipe with mulligans and priority windows (keep lean present)",
]
BIT_IDENTITY_BUILDS = "on the znver5 build and on the portable build, and on native arm64 when Mac arms use that build"
RECIPE_FIELD_RULE = ("A change that adds a recipe field also needs those rows with the field absent and the feature's "
                     "own differential tests with it present.")
TRAINER_RULE = ("A trainer pull request without those rows linked is not mergeable under the merge rule, "
                "whatever CI says.")
BLIND_NOTE = ("Production recipes do not exercise sideboarding or every observation path, so the bit-identity rows are "
              "blind there and these unit suites are the only gate; that is why the shared modules run in full.")
RULES_NOTE = ("Engine rules and card changes that alter play are never bit-identical by scope: outside the qualifying "
              "list they run the whole pull request set, and, if a running lineage's trainer would pick them up, need "
              "a declaration.")


def scope_of(path: str) -> str | None:
    for patterns, scope in PATH_RULES:
        if any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns):
            return scope
    return None


def scope_of_files(paths: list[str]) -> str:
    found = {scope_of(path) for path in paths} - NEUTRAL
    if not found:
        return "docs"
    if len(found) == 1:
        (only,) = found
        if only in SCOPES and only != "pr":
            return only
    return "pr"


def choose(event: str, group_set: str, input_scope: str, labels: list[str], files: list[str],
           merge_commits: int = 0) -> str:
    if event == "workflow_dispatch":
        return input_scope if input_scope in DISPATCH_SCOPES else "all"
    if event != "pull_request" or group_set != "pr" or "ci:full" in labels or merge_commits > 0:
        return "all"
    scope = scope_of_files(files)
    return "all" if scope == "pr" else scope


def trainer_record() -> str:
    return (f"release-trainer scope: CI runs formatting, Clippy and the library tests of the trainer and qualifying "
            f"shared modules only. Mandatory bit-identity rows, run on a node, not in CI: {'; '.join(BIT_IDENTITY_ROWS)}, "
            f"{BIT_IDENTITY_BUILDS}. {RECIPE_FIELD_RULE} {TRAINER_RULE}")


def markdown() -> str:
    out = ["# CI scopes (release line)", "",
           "Generated by `python tools/ci/ci_scope.py markdown` from `tools/ci/ci_scope.py`; "
           "`tools/ci/tests/test_ci_scope.py` fails if this file and the table differ.", "",
           "A ready pull request whose files all qualify for release-trainer runs only that scope. Any other file, "
           "any merge commit in the pull request (a merge between this line and main), or a pull request of more "
           "than 250 commits (the listing limit), runs the whole pull request set. Markdown files never decide. "
           "`ci:full` forces the whole set on the next push or ready (a label alone starts no run); labels never "
           "narrow a run. Every "
           "merge into the line runs the full matrix as a monitor; a red Limited or Eternal shard there is fixed "
           "forward or reverted the same day.", "",
           "## Files that qualify for release-trainer", "",
           "Trainer modules: " + ", ".join(f"`{p}`" for p in TRAINER_MODULES) + ".", "",
           "Shared modules (tested in full): " + ", ".join(f"`{p}`" for p in SHARED_QUALIFYING) + ".", "",
           "Everything else: whole pull request set (docs files: no checks).", "",
           "## Scope to checks", "", "| scope | ci.yml jobs | what runs |", "|---|---|---|"]
    for name, spec in SCOPES.items():
        out.append(f"| {name} | {', '.join(spec['jobs']) or 'none'} | {spec['summary']} |")
    out += ["", "Library test filters (default features): " + ", ".join(f"`{f}`" for f in TRAINER_FILTERS) + ".", ""]
    out += [f"Under `{features}`: " + ", ".join(f"`{f}`" for f in filters_for(features)) + "."
            for features in FEATURE_SETS if features]
    out += ["",
            trainer_record(), "", BLIND_NOTE, "", RULES_NOTE, ""]
    return "\n".join(out)


def main(argv: list[str]) -> int:
    command = argv[1] if len(argv) > 1 else ""
    if command == "markdown":
        sys.stdout.write(markdown())
        return 0
    if command == "trainer-filters":
        features = argv[3] if len(argv) > 3 and argv[2] == "--features" else ""
        print(" ".join(filters_for(features)))
        return 0
    if command == "file-scope":
        print(scope_of_files([line.strip() for line in sys.stdin if line.strip()]))
        return 0
    if command == "commands":
        if argv[2] != "release-trainer":
            print(f"no local command list for scope {argv[2]}", file=sys.stderr)
            return 2
        for features in FEATURE_SETS:
            print(f'python python/tools/run_rust_lib_filters.py --features "{features}" ' + " ".join(filters_for(features)))
        return 0
    if command == "choose":
        labels = json.loads(os.environ.get("LABELS") or "[]")
        files = [line for line in (os.environ.get("CHANGED_FILES") or "").splitlines() if line.strip()]
        checks = choose(os.environ.get("EVENT", ""), os.environ.get("SCOPE", ""), os.environ.get("INPUT_SCOPE", ""),
                        labels, files, int(os.environ.get("MERGE_COMMITS") or 0))
        lines = [f"checks: {checks}" + ("" if checks == "all" else f" ({SCOPES[checks]['summary']})")]
        if checks == "release-trainer":
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
