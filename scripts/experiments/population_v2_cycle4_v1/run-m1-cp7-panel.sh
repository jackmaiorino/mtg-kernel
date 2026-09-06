#!/usr/bin/env bash
# Cycle-4 M1 CP7 transfer panel (pre-registration section M1): five models on
# the same 2,048 common roots, both seat-swapped legs, harness of record
# (scorer v5 f5a9a0aa..., Mage lead/protocol-v2-reset-pin-v1 @ 72a08a3b,
# runner of record 7c0c0fb6... from the scorer-v3 worktree).
#
# The runner of record takes exactly three models with three DISTINCT Store
# roots per invocation, accepts exactly 128 pairs per formal run, and refuses
# an evidence root that already exists. So the panel is two model groups, each
# run as 16 disjoint 128-pair shards on ONE base seed (roots are common by
# construction: seed plus pair index, proven post hoc by the analyzer from the
# environment seeds), each shard in its own fresh leaf directory, with the
# void cap REPORTED per shard and ENFORCED over the full 2,048 roots by the
# analyzer (2 percent per model, the registered cap).
#
#   group A: treatment-rb@2048, control-r@2048, g896 (cycle-3 focal store @896)
#   group B: static-rb@2048, cycle3-g2048 (cycle-3 focal store @2048), treatment-rb@2048
#            (treatment-rb repeated: 16 shards that must reproduce group A's byte for byte,
#             the cross-invocation determinism check the analyzer enforces)
#
# FORMAL SEED LITERAL (launcher-level; a band no other measurement uses: the
# campaign panels use 4.1e9 to 5.3e9, the M2 panel 5.1e9, the CP7 shards
# 2026090199): M1_BASE_SEED = 2026090501.
#
#   usage: run-m1-cp7-panel.sh <a|b|both>
# Fail-closed: any shard exiting nonzero stops the launcher; a shard root that
# already exists stops the launcher before anything runs (move it outside the
# group root by hand and record why before retrying; the analyzer counts every
# shard-* directory it finds).
set -euo pipefail
WHICH="${1:-both}"
M1_BASE_SEED=2026090501
PAIRS_TOTAL=2048
SHARD_PAIRS=128
ARMS='E:\mtg-kernel-cycle4-arms-lead'
EVIDENCE="$ARMS\\cp7-evidence"
RUNNER='C:\Users\Jack\IdeaProjects\mtg-kernel-cp7-scorer-v3\scripts\current_net8_cp7_population_store_panel_v2\run_cp7_store_panel_v2.py'
SCORER='D:\cargo-target-cp7-scorer-v5\release\checkpoint_shadow_stdio_v1-f5a9a0aa.exe'
MAGE='C:\Users\Jack\IdeaProjects\mage-cycle4-lead'
CARDDB='E:\mtg-kernel-population-v2-cycle3-cp7-anchor-reads\carddb-staging\cards.h2.mv.db'
MAVEN='C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd'
PY='C:\Users\Jack\AppData\Local\Programs\Python\Python311\python.exe'
CYCLE3='E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003\run-0\store'

expected_scorer=f5a9a0aa95f9a4f823d23a5e06f29b8c1626e427f51b15c7769c2aaed6d3de6d
expected_runner=7c0c0fb68c814dcda20086caf9201550c5ae0b35e78e6d8d7feb5716927fc9dd
expected_mage=72a08a3b2654df26bba7bcd7c716885a1fb89174
# The runner takes Windows-form paths; the shell-side guards need POSIX form.
posix() { cygpath -u "$1"; }
actual_scorer=$(sha256sum "$(posix "$SCORER")" | cut -c1-64)
actual_runner=$(sha256sum "$(posix "$RUNNER")" | cut -c1-64)
[ "$actual_scorer" = "$expected_scorer" ] || { echo "scorer hash mismatch: $actual_scorer"; exit 3; }
[ "$actual_runner" = "$expected_runner" ] || { echo "runner hash mismatch: $actual_runner"; exit 3; }
[ "$(git -C "$(posix "$MAGE")" rev-parse HEAD)" = "$expected_mage" ] || { echo "mage commit mismatch"; exit 3; }
[ -z "$(git -C "$(posix "$MAGE")" status --porcelain)" ] || { echo "mage worktree dirty"; exit 3; }
[ -f "$(posix "$CARDDB")" ] || { echo "card database missing"; exit 3; }
echo "harness of record verified: scorer $actual_scorer runner $actual_runner mage $expected_mage"

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
    if [ -e "$(cygpath -u "$leaf")" ]; then echo "shard root already exists: $leaf"; exit 3; fi
  done
}

run_group() {
  local tag="$1"; shift
  local group_root="$EVIDENCE\\m1-$tag"
  for (( s = 0; s < shard_count; s++ )); do
    local leaf; leaf=$(printf '%s\\shard-%02d' "$group_root" "$s")
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
