"""Launch a Spy execution diagnosis queue on this host: take the host
reservation (host_reservation_v1), then run diag_queue.py under
`host_slots_v1.py timed --cores <declared>`, which pins the queue and every
descendant to the declared cores and leaves the rest of the host to untimed
claims. Run it on the host that does the work (HaleysPC over ssh).

Usage: python launch_diag.py --root DIR --binary EXE --queue QUEUE.json
         --cores SPEC --tools DIR --slots PATH [--work-id ID]
Prints the dispatch record (state, pid, token generation).
"""
import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True)
    ap.add_argument("--binary", required=True)
    ap.add_argument("--queue", required=True)
    ap.add_argument("--cores", required=True)
    ap.add_argument("--tools", required=True, help="dir holding host_reservation_v1.py")
    ap.add_argument("--slots", required=True, help="host_slots_v1.py")
    ap.add_argument("--work-id", default="spy-diagnosis")
    a = ap.parse_args(argv)
    sys.path.insert(0, a.tools)
    import host_reservation_v1 as reservations
    cmd = [sys.executable, "-B", a.slots, "timed", "--cores", a.cores, "--",
           sys.executable, "-B", str(HERE / "diag_queue.py"), a.root, a.binary, a.queue]
    r = reservations.dispatch(lane="claude-spy-execution-diagnosis-20261010", work_id=a.work_id,
                              release_condition="spy diagnosis queue complete or failed", command=cmd, cwd=a.root,
                              busy_pattern=r"regret_census_v1|native_expanded_training")
    print(json.dumps({k: r.get(k) for k in ("state", "nested", "pid", "generation")}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
