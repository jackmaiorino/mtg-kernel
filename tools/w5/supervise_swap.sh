#!/bin/bash
# Steps for moving a running `chains.py supervise --loop` to another W5 tools tree (docs/W5-SUPERVISE-MEMORY-20261120.md).
# One source: the procedure runs these steps and tests/supervise_swap_rehearsal.py runs the same file.
#
#   C=... ROOT=... P=... PY=... bash supervise_swap.sh STEP [ARGS]
#
# Environment: C (the lane directory; records/ under it), ROOT (the plan root, default $C/oc), PY (absolute python),
# LOOP (supervise --loop seconds, default 60), PAUSE (restart pause, default 30).
# Steps:
#   plan_copy PLAN PLAN2 NEW_W5        the supervisor's plan copy: only `tools` changes (three files), pins checked
#   start_loop W5 PLAN [--guard-only]   the restart loop (absolute python, exit code logged); prints the loop pid
#   pids PLAN                           "LOOP_PIDS ... ; PYTHON_PIDS ..." of the loops running PLAN
#   asleep PID SUPERVISE_JSON           the supervisor sleeps between cycles (state S, no CPU in 2 s, report < 10 s old)
#   stop_between_cycles PLAN            waits for a cycle's end, checks asleep, stops loop then supervisor, verifies
#   stop_failing PLAN                   bounded stop of a loop that may never publish: loop, then supervisor, SIGKILL
#                                       after 20 s, verifies nothing of PLAN is left
#   check_tails                         every chain index ends in a newline and its last line parses
#   cut_partial_tail INDEX              cuts an unterminated, unparseable last line back to the previous newline
set -u
C=${C:?}; ROOT=${ROOT:-$C/oc}; PY=${PY:?}; LOOP=${LOOP:-60}; PAUSE=${PAUSE:-30}
RECORDS=$C/records

say() { echo "$(date -Is) $*"; }

pids() {  # loop shells and supervisor pythons running PLAN (bash or python command lines only)
    local plan=$1 loops="" pys="" p comm
    for p in $(pgrep -f -- "supervise --plan $plan( |$)"); do
        comm=$(cat /proc/$p/comm 2>/dev/null) || continue
        case $comm in
            bash) loops="$loops $p" ;;
            python*) pys="$pys $p" ;;
        esac
    done
    echo "LOOP_PIDS$loops ; PYTHON_PIDS$pys"
}

alive() {  # the pids of $@ that still exist and are not zombies
    local p out=""
    for p in "$@"; do
        [ -r /proc/$p/stat ] || continue
        [ "$(awk '{print $3}' /proc/$p/stat 2>/dev/null)" = Z ] && continue
        out="$out $p"
    done
    echo $out
}

cpu_ticks() { awk '{print $14 + $15}' /proc/$1/stat; }

asleep() {
    local pid=$1 sup=$2 state t0 t1 age
    state=$(awk '{print $3}' /proc/$pid/stat 2>/dev/null) || { say "asleep: $pid is gone"; return 1; }
    t0=$(cpu_ticks $pid); sleep 2; t1=$(cpu_ticks $pid)
    age=$(( $(date +%s) - $(stat -c %Y "$sup") ))
    say "asleep: pid $pid state $state cpu_ticks $t0 -> $t1 report_age ${age}s"
    [ "$state" = S ] && [ "$t0" = "$t1" ] && [ "$age" -lt 10 ]
}

kill_and_verify() {  # SIGTERM the loops, then the supervisors; SIGKILL after 20 s; nothing may be left
    local loops=$1 pys=$2 left i
    [ -n "$loops" ] && kill $loops 2>/dev/null
    [ -n "$pys" ] && kill $pys 2>/dev/null
    for i in $(seq 1 20); do
        left=$(alive $loops $pys)
        [ -z "$left" ] && break
        sleep 1
    done
    left=$(alive $loops $pys)
    if [ -n "$left" ]; then
        say "SIGKILL $left"
        kill -9 $left 2>/dev/null; sleep 1
        left=$(alive $loops $pys)
    fi
    [ -z "$left" ] || { say "STILL ALIVE: $left"; return 1; }
}

stop_between_cycles() {
    local plan=$1 sup line loops pys try t0
    sup=$(dirname "$plan")/SUPERVISE.json
    line=$(pids "$plan"); loops=${line#LOOP_PIDS}; loops=${loops%%;*}; pys=${line##*PYTHON_PIDS}
    say "stop_between_cycles: loops [$loops] supervisors [$pys]"
    [ -n "$(echo $pys)" ] || { say "no supervisor running $plan"; return 1; }
    for try in 1 2 3; do
        t0=$(stat -c %Y "$sup")
        timeout $(( LOOP * 3 + 600 )) bash -c "while [ \"\$(stat -c %Y '$sup')\" = '$t0' ]; do sleep 1; done" \
            || { say "no cycle ended in time: use stop_failing"; return 1; }
        sleep 2
        asleep $(echo $pys | cut -d" " -f1) "$sup" && break
        [ $try = 3 ] && { say "not asleep after three cycles: stop"; return 1; }
    done
    kill_and_verify "$loops" "$pys" || return 1
    line=$(pids "$plan"); [ "$line" = "LOOP_PIDS ; PYTHON_PIDS" ] || { say "left: $line"; return 1; }
    say "stopped: $plan"
}

stop_failing() {
    local plan=$1 line loops pys
    line=$(pids "$plan"); loops=${line#LOOP_PIDS}; loops=${loops%%;*}; pys=${line##*PYTHON_PIDS}
    say "stop_failing: loops [$loops] supervisors [$pys]"
    kill_and_verify "$loops" "$pys" || return 1
    line=$(pids "$plan")
    if [ "$line" != "LOOP_PIDS ; PYTHON_PIDS" ]; then  # a supervisor the loop restarted in between
        loops=${line#LOOP_PIDS}; loops=${loops%%;*}; pys=${line##*PYTHON_PIDS}
        kill_and_verify "$loops" "$pys" || return 1
    fi
    line=$(pids "$plan"); [ "$line" = "LOOP_PIDS ; PYTHON_PIDS" ] || { say "left: $line"; return 1; }
    say "stopped: $plan"
}

start_loop() {
    local w5=$1 plan=$2 extra=${3:-}
    setsid nohup bash -c "while true; do $PY $w5/chains.py supervise --plan $plan --loop $LOOP $extra >> $RECORDS/supervise.log 2>&1; rc=\$?; echo \"\$(date -Is) supervise exited \$rc\" >> $RECORDS/supervise-restarts.log; sleep $PAUSE; done" > /dev/null 2>&1 < /dev/null &
    local loop=$!
    sleep 2
    [ "$(cat /proc/$loop/comm 2>/dev/null)" = bash ] || { say "the loop did not start"; return 1; }
    say "started: loop $loop $(pids "$plan")"
    echo "LOOP_PID $loop"
}

plan_copy() {
    $PY -B - "$1" "$2" "$3" <<'EOF'
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

check_tails() {
    local f bad=0 last
    for f in "$ROOT"/chains/*/chain.index.jsonl; do
        last=$(tail -c 1 "$f" | od -An -c | tr -d ' ')
        if [ "$last" != '\n' ]; then
            say "BAD no final newline: $f"; bad=1; continue
        fi
        if tail -n 1 "$f" | $PY -c 'import json,sys; json.loads(sys.stdin.read())' 2>/dev/null; then
            say "ok $f"
        else
            say "BAD last line does not parse: $f"; bad=1
        fi
    done
    return $bad
}

cut_partial_tail() {  # only an unterminated last line that does not parse; a whole line without its newline is
    local f=$1 keep   # left alone (index() reads it and the next append terminates it)
    keep=$($PY -B - "$f" <<'EOF'
import json, sys
data = open(sys.argv[1], 'rb').read()
if not data or data.endswith(b'\n'):
    sys.exit('nothing to cut: the file ends in a newline')
cut = data.rfind(b'\n') + 1
try:
    json.loads(data[cut:].decode('utf-8'))
    sys.exit('the last line parses: leave it (the next append adds its newline)')
except ValueError:
    print(cut)
EOF
) || return 1
    say "cut $f from $(stat -c %s "$f") to $keep bytes"
    truncate -s "$keep" "$f"
}

step=$1; shift
"$step" "$@"
