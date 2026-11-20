#!/bin/bash
# Steps for moving a running `chains.py supervise --loop` to another W5 tools tree (docs/W5-SUPERVISE-MEMORY-20261120.md).
# One source: the procedure runs these steps and tests/supervise_swap_rehearsal.py runs the same file. The helpers
# (process identity, index tails, the watch) are in supervise_swap.py beside this file.
#
#   C=... ROOT=... PY=... bash supervise_swap.sh STEP [ARGS]
#
# Environment: C (the lane directory; records/ under it), ROOT (the plan root, default $C/oc), PY (absolute python),
# LOOP (supervise --loop seconds, default 60), PAUSE (restart pause, default 30).
# Steps:
#   plan_copy PLAN PLAN2 NEW_W5        the supervisor's plan copy: only `tools` changes (three files), pins checked
#   start_loop W5 PLAN [--guard-only]   refuses if PLAN already runs; checks every index tail; starts the loop (paths as
#                                       arguments, exit code logged); prints LOOP_PID
#   pids PLAN                           "LOOP_PIDS ... ; PYTHON_PIDS ..." running exactly PLAN (argv compared exactly)
#   asleep PID SUPERVISE_JSON           the supervisor sleeps between cycles (state S, no CPU in 2 s, report < 10 s old)
#   stop_between_cycles PLAN            exactly one loop and one supervisor; waits for a cycle's end, checks asleep,
#                                       stops loop then supervisor (each re-verified before its signal), verifies
#   stop_failing PLAN                   bounded stop of a loop that may never publish: loop, then supervisor, SIGKILL
#                                       after 20 s, verifies nothing of PLAN is left
#   check_tails                         every chain index's last line parses (a missing final newline is fine)
#   cut_partial_tail INDEX              under the chain's index lock: an unparseable unterminated last line is saved
#                                       to records/ and cut back to the previous newline
#   watch_swap PLAN T0 [EVERY] [ROUNDS] read only: every EVERY s (30) GUARD age, each live chain's admitted count,
#                                       ceiling and guard wait, seconds to the escalation time; exits 0 on SUCCESS,
#                                       2 on ESCALATE, 1 after ROUNDS lines of neither
set -u
C=${C:?}; ROOT=${ROOT:-$C/oc}; PY=${PY:?}; LOOP=${LOOP:-60}; PAUSE=${PAUSE:-30}
RECORDS=$C/records
HELPER=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/supervise_swap.py
GUARD_ONLY_SECONDS=${GUARD_ONLY_SECONDS:-723}

say() { echo "$(date -Is) $*"; }

[ "$LOOP" -ge 10 ] || { say "LOOP must be at least 10 s (asleep bounds the report age by LOOP - 5)"; exit 1; }

pids() { "$PY" -B "$HELPER" pids "$1"; }

split_pids() {  # sets LOOPS and PYS (trimmed: no pid is an empty list) from `pids PLAN`
    local line
    line=$(pids "$1") || return 1
    LOOPS=${line#LOOP_PIDS}; LOOPS=${LOOPS%%;*}; PYS=${line##*PYTHON_PIDS}
    LOOPS=$(echo $LOOPS); PYS=$(echo $PYS)
}

count() { set -- $1; echo $#; }

alive() {  # the pids of $@ that still exist and are not zombies
    local p out=""
    for p in "$@"; do
        [ -r /proc/$p/stat ] || continue
        [ "$(awk '{print $3}' /proc/$p/stat 2>/dev/null)" = Z ] && continue
        out="$out $p"
    done
    echo $out
}

proc_state() { awk '{print $3}' /proc/$1/stat 2>/dev/null; }
cpu_ticks() { awk '{print $14 + $15}' /proc/$1/stat 2>/dev/null; }

asleep() {  # state S before and after 2 s, the same CPU ticks, a report written 0 to LOOP - 5 s ago
    local pid=$1 sup=$2 s0 s1 t0 t1 age
    s0=$(proc_state $pid); t0=$(cpu_ticks $pid); sleep 2; s1=$(proc_state $pid); t1=$(cpu_ticks $pid)
    age=$(( $(date +%s) - $(stat -c %Y "$sup") ))
    say "asleep: pid $pid state $s0 -> $s1 cpu_ticks $t0 -> $t1 report_age ${age}s (bound 0 to $(( LOOP - 5 )))"
    [ -n "$t0" ] && [ -n "$t1" ] && [ "$s0" = S ] && [ "$s1" = S ] && [ "$t0" = "$t1" ] \
        && [ "$age" -ge 0 ] && [ "$age" -lt $(( LOOP - 5 )) ]
}

kill_and_verify() {  # PLAN LOOPS PYS: SIGTERM the loops, then the supervisors, each re-verified (its argv for PLAN)
    local plan=$1 loops=$2 pys=$3 left i       # just before the signal; SIGKILL after 20 s; nothing may be left
    [ -n "$loops" ] && "$PY" -B "$HELPER" signal "$plan" 15 $loops
    [ -n "$pys" ] && "$PY" -B "$HELPER" signal "$plan" 15 $pys
    for i in $(seq 1 20); do
        left=$(alive $loops $pys)
        [ -z "$left" ] && break
        sleep 1
    done
    left=$(alive $loops $pys)
    if [ -n "$left" ]; then
        say "SIGKILL $left"
        "$PY" -B "$HELPER" signal "$plan" 9 $left; sleep 1
        left=$(alive $loops $pys)
    fi
    [ -z "$left" ] || { say "STILL ALIVE: $left"; return 1; }
}

stop_between_cycles() {  # requires exactly one loop and one supervisor for PLAN, read again on every try
    local plan=$1 sup try t0 line
    sup=$(dirname "$plan")/SUPERVISE.json
    for try in 1 2 3; do
        split_pids "$plan" || return 1
        say "stop_between_cycles: loops [$LOOPS] supervisors [$PYS]"
        [ "$(count "$LOOPS")" = 1 ] && [ "$(count "$PYS")" = 1 ] \
            || { say "refused: need exactly one loop and one supervisor for $plan"; return 1; }
        t0=$(stat -c %Y "$sup")
        timeout $(( LOOP * 3 + 600 )) bash -c 'while [ "$(stat -c %Y "$1")" = "$2" ]; do sleep 1; done' _ "$sup" "$t0" \
            || { say "no cycle ended in time: use stop_failing"; return 1; }
        sleep 2
        split_pids "$plan" || return 1
        [ "$(count "$LOOPS")" = 1 ] && [ "$(count "$PYS")" = 1 ] || continue
        asleep $PYS "$sup" && break
        [ $try = 3 ] && { say "not asleep after three cycles: stop"; return 1; }
    done
    kill_and_verify "$plan" "$LOOPS" "$PYS" || return 1
    line=$(pids "$plan"); [ "$line" = "LOOP_PIDS ; PYTHON_PIDS" ] || { say "left: $line"; return 1; }
    say "stopped: $plan (run check_tails before any start)"
}

stop_failing() {
    local plan=$1 line
    split_pids "$plan" || return 1
    say "stop_failing: loops [$LOOPS] supervisors [$PYS]"
    [ -n "$LOOPS$PYS" ] || { say "refused: nothing runs $plan"; return 1; }
    kill_and_verify "$plan" "$LOOPS" "$PYS" || return 1
    line=$(pids "$plan")
    if [ "$line" != "LOOP_PIDS ; PYTHON_PIDS" ]; then  # a supervisor the loop restarted in between
        split_pids "$plan"
        kill_and_verify "$plan" "$LOOPS" "$PYS" || return 1
    fi
    line=$(pids "$plan"); [ "$line" = "LOOP_PIDS ; PYTHON_PIDS" ] || { say "left: $line"; return 1; }
    say "stopped: $plan (run check_tails before any start)"
}

check_tails() { "$PY" -B "$HELPER" check-tails "$ROOT"; }

cut_partial_tail() { "$PY" -B "$HELPER" cut-tail "$1" "$RECORDS"; }

start_loop() {
    local w5=$1 plan=$2 extra=${3:-} text line
    line=$(pids "$plan")
    [ "$line" = "LOOP_PIDS ; PYTHON_PIDS" ] || { say "refused: $plan already runs: $line"; return 1; }
    check_tails || { say "an index's last line does not parse: cut_partial_tail it first"; return 1; }
    text=$("$PY" -B "$HELPER" loop-text) || return 1
    setsid nohup bash -c "$text" swap-loop "$PY" "$w5" "$plan" "$LOOP" "$RECORDS" "$PAUSE" "$extra" \
        > /dev/null 2>&1 < /dev/null &
    local loop=$!
    sleep 2
    [ "$(cat /proc/$loop/comm 2>/dev/null)" = bash ] || { say "the loop did not start"; return 1; }
    line=$(pids "$plan")
    case " ${line%%;*} " in *" $loop "*) ;; *) say "the loop $loop is not listed for $plan: $line"; return 1 ;; esac
    say "started: loop $loop $line"
    echo "LOOP_PID $loop"
}

plan_copy() {
    "$PY" -B - "$1" "$2" "$3" <<'EOF'
import json, sys
sys.path.insert(0, sys.argv[3]); import chains
plan = json.load(open(sys.argv[1]))
new = chains.tools_shas()
changed = sorted(k for k in new if plan['tools'].get(k) != new[k])
assert changed == ['chains.py', 'replay_sample.py', 'retention.py'] and set(new) == set(plan['tools']), changed
print(changed)
plan['tools'] = new
with open(sys.argv[2], 'x') as f:
    f.write(json.dumps(plan, indent=1) + '\n')
chains.load_plan(sys.argv[2])
print('ok')
EOF
}

watch_swap() {
    "$PY" -B "$HELPER" watch "$ROOT" "$1" "$2" "$GUARD_ONLY_SECONDS" --every "${3:-30}" --log "$RECORDS/supervise.log"         ${4:+--rounds "$4"}
}

step=$1; shift
"$step" "$@"
