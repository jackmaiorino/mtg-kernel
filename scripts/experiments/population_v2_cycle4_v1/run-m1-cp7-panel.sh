#!/usr/bin/env bash
# Cycle-4 M1 CP7 transfer panel (pre-registration section M1): five models on
# the same 2,048 common roots, both seat-swapped legs, harness of record
# (scorer v4 137b3d0a..., Mage lead/protocol-v2-reset-pin-v1 @ 72a08a3b,
# runner of record 7c0c0fb6... from the scorer-v3 worktree). The runner takes
# exactly three models per invocation, so the panel is two invocations on the
# SAME base seed and pair range; roots are common by construction (seed plus
# pair index) and the analyzer proves it post hoc from the environment seeds.
#
#   The runner also requires three DISTINCT Store roots per invocation, so the
#   two cycle-3 models (g896 and g2048 read from the same focal Store) cannot
#   share one; treatment-rb, the primary endpoint, is repeated instead as the
#   cross-invocation determinism check.
#   invocation A: treatment-rb@2048, control-r@2048, g896 (cycle-3 focal store @896)
#   invocation B: static-rb@2048, cycle3-g2048 (cycle-3 focal store @2048), treatment-rb@2048 (repeat)
#
# FORMAL SEED LITERAL (launcher-level, disjoint from every other band in the
# program; the campaign's payoff panels use 4.1e9 to 5.3e9 bands, the M2 panel
# 5.1e9, the CP7 shards 2026090199): M1_BASE_SEED = 2026090501.
#   usage: run-m1-cp7-panel.sh <a|b|both>
set -euo pipefail
WHICH="${1:-both}"
M1_BASE_SEED=2026090501
PAIRS=2048
ARMS='E:\mtg-kernel-cycle4-arms-lead'
EVIDENCE="$ARMS\\cp7-evidence"
RUNNER='C:\Users\Jack\IdeaProjects\mtg-kernel-cp7-scorer-v3\scripts\current_net8_cp7_population_store_panel_v2\run_cp7_store_panel_v2.py'
SCORER='D:\cargo-target-cp7-scorer-v4\release\checkpoint_shadow_stdio_v1.exe'
MAGE='C:\Users\Jack\IdeaProjects\mage-cycle4-lead'
CARDDB='E:\mtg-kernel-population-v2-cycle3-cp7-anchor-reads\carddb-staging\cards.h2.mv.db'
MAVEN='C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd'
PY='C:\Users\Jack\AppData\Local\Programs\Python\Python311\python.exe'
CYCLE3='E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003\run-0\store'

expected_scorer=137b3d0a3ccc93fea92567200494ce2cd5f097be74c68fab28977ab7a44a0677
expected_runner=7c0c0fb68c814dcda20086caf9201550c5ae0b35e78e6d8d7feb5716927fc9dd
[ "$(sha256sum "$SCORER" | cut -c1-64)" = "$expected_scorer" ] || { echo "scorer hash mismatch"; exit 3; }
[ "$(sha256sum "$RUNNER" | cut -c1-64)" = "$expected_runner" ] || { echo "runner hash mismatch"; exit 3; }
[ "$(git -C "$MAGE" rev-parse HEAD)" = "72a08a3b2654df26bba7bcd7c716885a1fb89174" ] || { echo "mage commit mismatch"; exit 3; }
[ -z "$(git -C "$MAGE" status --porcelain)" ] || { echo "mage worktree dirty"; exit 3; }

run_one() {
  local tag="$1"; shift
  local root="$EVIDENCE\\m1-$tag"
  mkdir -p "$(cygpath -u "$root")"
  date -u +"M1-$tag START %Y-%m-%dT%H:%M:%SZ"
  "$PY" "$RUNNER" \
    --evidence-root "$root" \
    "$@" \
    --mode formal \
    --base-seed "$M1_BASE_SEED" \
    --pair-start 0 \
    --pairs "$PAIRS" \
    --void-cap-mode enforce \
    --workers 8 \
    --task-pairs 32 \
    --task-timeout-seconds 1800 \
    --scorer-exe "$SCORER" \
    --mage-repo "$MAGE" \
    --source-database "$CARDDB" \
    --maven "$MAVEN" \
    --tolerate-engine-faults
  local rc=$?
  date -u +"M1-$tag END %Y-%m-%dT%H:%M:%SZ RETURNCODE $rc"
  return $rc
}

A=( --model "treatment-rb=population:2048:$ARMS\\treatment-rb\\store"
    --model "control-r=population:2048:$ARMS\\control-r\\store"
    --model "g896=population:896:$CYCLE3" )
B=( --model "static-rb=population:2048:$ARMS\\static-rb\\store"
    --model "cycle3-g2048=population:2048:$CYCLE3"
    --model "treatment-rb=population:2048:$ARMS\\treatment-rb\\store" )

case "$WHICH" in
  a) run_one a "${A[@]}" ;;
  b) run_one b "${B[@]}" ;;
  both) run_one a "${A[@]}"; run_one b "${B[@]}" ;;
  *) echo "usage: $0 <a|b|both>"; exit 2 ;;
esac
