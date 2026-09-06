#!/usr/bin/env bash
# Resume variant of run-m1-cp7-panel.sh for a panel that was yielded at a shard
# boundary (CODEX #81 resource priority, 2026-09-06). Identical harness, guards,
# seed literal, groups and per-shard runner invocation; the only difference is
# that a shard leaf that already holds a completed panel-summary.json is SKIPPED
# (the shard is one runner invocation on a fixed seed and pair range, so a
# completed leaf is content-identical to what a fresh run would produce), while
# a leaf that exists WITHOUT a panel-summary.json (a shard interrupted mid-run)
# stops the launcher fail-closed: move it aside by hand outside the group root
# and record why before retrying. No shard is ever partially reused.
#
#   usage: run-m1-cp7-panel-resume.sh <a|b|both>
set -euo pipefail
WHICH="${1:-both}"
M1_BASE_SEED=2026090501
PAIRS_TOTAL=2048
SHARD_PAIRS=128
ARMS='E:\mtg-kernel-cycle4-arms-lead'
EVIDENCE="$ARMS\\cp7-evidence"
RUNNER='C:\Users\Jack\IdeaProjects\mtg-kernel-cp7-scorer-v3\scripts\current_net8_cp7_population_store_panel_v2\run_cp7_store_panel_v2.py'
SCORER='D:\cargo-target-cp7-scorer-v5\release\checkpoint_shadow_stdio_v1-f5a9a0aa.exe'
MAGE='C:\Users\Jack\IdeaProjects\mage-cp7-mapper-fix'
CARDDB='E:\mtg-kernel-population-v2-cycle3-cp7-anchor-reads\carddb-staging\cards.h2.mv.db'
MAVEN='C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd'
PY='C:\Users\Jack\AppData\Local\Programs\Python\Python311\python.exe'
CYCLE3='E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003\run-0\store'

expected_scorer=f5a9a0aa95f9a4f823d23a5e06f29b8c1626e427f51b15c7769c2aaed6d3de6d
expected_runner=7c0c0fb68c814dcda20086caf9201550c5ae0b35e78e6d8d7feb5716927fc9dd
expected_mage=f89c68fc1f08aca79cfa3f990e965f31c61b7086
posix() { cygpath -u "$1"; }
actual_scorer=$(sha256sum "$(posix "$SCORER")" | cut -c1-64)
actual_runner=$(sha256sum "$(posix "$RUNNER")" | cut -c1-64)
[ "$actual_scorer" = "$expected_scorer" ] || { echo "scorer hash mismatch: $actual_scorer"; exit 3; }
[ "$actual_runner" = "$expected_runner" ] || { echo "runner hash mismatch: $actual_runner"; exit 3; }
[ "$(git -C "$(posix "$MAGE")" rev-parse HEAD)" = "$expected_mage" ] || { echo "mage commit mismatch"; exit 3; }
[ -z "$(git -C "$(posix "$MAGE")" status --porcelain)" ] || { echo "mage worktree dirty"; exit 3; }
[ -f "$(posix "$CARDDB")" ] || { echo "card database missing"; exit 3; }
echo "harness of record verified: scorer $actual_scorer runner $actual_runner mage $expected_mage (resume launcher)"

A=( --model "treatment-rb=population:2048:$ARMS\\treatment-rb\\store"
    --model "control-r=population:2048:$ARMS\\control-r\\store"
    --model "g896=population:896:$CYCLE3" )
B=( --model "static-rb=population:2048:$ARMS\\static-rb\\store"
    --model "cycle3-g2048=population:2048:$CYCLE3"
    --model "treatment-rb=population:2048:$ARMS\\treatment-rb\\store" )

shard_count=$(( PAIRS_TOTAL / SHARD_PAIRS ))

precheck_group() {
  local tag="$1"
  local group_root="$EVIDENCE\\m1-$tag"
  mkdir -p "$(cygpath -u "$group_root")"
  for (( s = 0; s < shard_count; s++ )); do
    local leaf; leaf=$(printf '%s\\shard-%02d' "$group_root" "$s")
    local pleaf; pleaf=$(cygpath -u "$leaf")
    if [ -e "$pleaf" ] && [ ! -f "$pleaf/panel-summary.json" ]; then
      echo "shard root exists without a completed panel-summary.json (interrupted shard): $leaf"; exit 3
    fi
  done
}

run_group() {
  local tag="$1"; shift
  local group_root="$EVIDENCE\\m1-$tag"
  for (( s = 0; s < shard_count; s++ )); do
    local leaf; leaf=$(printf '%s\\shard-%02d' "$group_root" "$s")
    local pleaf; pleaf=$(cygpath -u "$leaf")
    if [ -f "$pleaf/panel-summary.json" ]; then
      date -u +"M1-$tag shard $s SKIP (completed before this resume) %Y-%m-%dT%H:%M:%SZ"
      continue
    fi
    local pair_start=$(( s * SHARD_PAIRS ))
    date -u +"M1-$tag shard $s pairs $pair_start..$(( pair_start + SHARD_PAIRS - 1 )) START %Y-%m-%dT%H:%M:%SZ"
    "$PY" "$RUNNER" \
      --evidence-root "$leaf" \
      "$@" \
      --mode formal \
      --base-seed "$M1_BASE_SEED" \
      --pair-start "$pair_start" \
      --pairs "$SHARD_PAIRS" \
      --read-pairs "$PAIRS_TOTAL" \
      --void-cap-mode report \
      --workers 8 \
      --task-pairs 32 \
      --task-timeout-seconds 1800 \
      --scorer-exe "$SCORER" \
      --mage-repo "$MAGE" \
      --source-database "$CARDDB" \
      --maven "$MAVEN" \
      --tolerate-engine-faults
    local rc=$?
    date -u +"M1-$tag shard $s END %Y-%m-%dT%H:%M:%SZ RETURNCODE $rc"
    [ "$rc" -eq 0 ] || { echo "M1-$tag shard $s failed; launcher stopped"; exit "$rc"; }
  done
}

case "$WHICH" in
  a) precheck_group a; run_group a "${A[@]}" ;;
  b) precheck_group b; run_group b "${B[@]}" ;;
  both) precheck_group a; precheck_group b; run_group a "${A[@]}"; run_group b "${B[@]}" ;;
  *) echo "usage: $0 <a|b|both>"; exit 2 ;;
esac
date -u +"M1 $WHICH COMPLETE %Y-%m-%dT%H:%M:%SZ"
