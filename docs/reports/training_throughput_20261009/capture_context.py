"""Project explicit local audit snapshots to compact non-outcome metadata."""
from datetime import datetime
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SCRATCH = Path("C:/Users/Jack/.codex/temporary/training-throughput-audit-20261009/resource-agent")


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def pin(path):
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def stamp(value):
    return datetime.fromisoformat(value).timestamp()


def main():
    inventory = {}
    for host, name in (("desktop", "desktop-snapshot.json"), ("computehost", "haley-snapshot.json")):
        p = SCRATCH/name
        raw = load(p)
        inventory[host] = {k: raw[k] for k in ("observed_utc", "cpu", "memory", "gpu", "volumes", "disks", "local_reservation") if k in raw}
        # Reservation paths differ by inventory adapter; preserve only concise
        # identity, timing and release conditions, not process command lines.
        for key in ("reservation", "local_reservation"):
            if key in raw:
                inventory[host]["reservation"] = {k: raw[key][k] for k in ("generation", "lane", "work_id", "acquired_at", "release_condition") if k in raw[key]}
        inventory[host]["source"] = pin(p)
    p = SCRATCH/"runpod-snapshot.json"
    raw = load(p)
    inventory["runpod"] = {"observed_utc": raw["observed_utc"], "http_status": raw["http_status"],
                           "pod_count": raw["pod_count"], "statuses": [r["desiredStatus"] for r in raw["pods"]],
                           "new_paid_authority": False, "source": pin(p)}
    remote = load(SCRATCH/"haley-state-timing.json")
    states = {r["run"]: r for r in remote["runs"]}
    sources = [pin(SCRATCH/"haley-state-timing.json")]
    for run in ("r4", "r5", "r6"):
        p = Path("D:/nine-deck-baseline-20261007/campaign/state/runs")/f"{run}.json"
        raw = load(p)
        states[run] = {"updated": raw["updated"], "status": raw["status"],
                       "blocks": [{"block_key": k, "attempts": [{x: a[x] for x in ("attempt", "started", "finished", "outcome") if x in a} for a in v["attempts"]]} for k,v in raw["blocks"].items()]}
        sources.append(pin(p))
    timing = {}
    for run, state in states.items():
        attempts = [a for b in state["blocks"] for a in b["attempts"]]
        complete = [a for a in attempts if a.get("outcome") == "complete"]
        assert len(complete) == 20 and state["status"] == "complete"
        start = min(a["started"] for a in attempts if "started" in a)
        last = max(a["finished"] for a in complete)
        timing[run] = {"started": start, "last_successful_attempt_finished": last,
                       "complete_state_updated": state["updated"],
                       "calendar_envelope_seconds": stamp(state["updated"])-stamp(start),
                       "successful_attempt_seconds_sum": sum(stamp(a["finished"])-stamp(a["started"]) for a in complete),
                       "attempts": len(attempts), "completed_blocks": len(complete)}
    first = min(r["started"] for r in timing.values())
    last = max(r["complete_state_updated"] for r in timing.values())
    context = {"inventory": inventory, "state_sources": sources, "run_timing": timing,
               "calendar": {"first_dispatch": first, "last_complete_state": last,
                            "envelope_seconds": stamp(last)-stamp(first),
                            "scope": "Recorded first training dispatch through last completed training state; includes maintenance, interruptions, host scheduling and idle gaps; excludes prelaunch qualification and later evaluation."}}
    # These are a survivor subset. Missing pruned archives are not zero cost.
    archives = []
    for b in load(ROOT/"desktop-profile.json")["blocks"]:
        path = Path("E:/nine-deck-baseline-20261007/campaign/cold")/b["run"]/b["attempt"]/"archive.json"
        if path.exists():
            raw = load(path)
            report = load(Path(b["inputs"]["report_path"]))
            assert pin(path)["sha256"] == report["archive"]["sha256"]
            archives.append({"run": b["run"], "block": b["block"], "archive_seconds": raw["seconds"],
                             "post_execution_seconds": b["post_execution_seconds"], "source": pin(path)})
    context["surviving_desktop_archive_subset"] = archives
    remote_archives = load(ROOT/"haley-archive-timings.json")
    context["haley_archive_metadata"] = remote_archives
    recovered = []
    for a in remote_archives["blocks"]:
        if a["run"] == "r1" and not a["present"]:
            p = Path("E:/nine-deck-baseline-20261007/computehost-campaign/cold/r1")/a["block"]/"archive.json"
            assert pin(p)["sha256"] == a["archive_pin"]["sha256"]
            raw = load(p)
            recovered.append({"run": "r1", "block": a["block"], "seconds": raw["seconds"], "source": pin(p)})
    context["recovered_r1_archives"] = recovered
    assert len(archives) == 57 and len(recovered) == 20
    verified_remote = [a for a in remote_archives["blocks"] if a["present"] and a["verified"]]
    assert len(verified_remote) == 43
    context["archive_summary"] = {"blocks": 120, "seconds": sum(a["archive_seconds"] for a in archives)+sum(a["seconds"] for a in recovered)+sum(a["archive_seconds"] for a in verified_remote)}
    qualifications = []
    for workers in (1,2,4,8):
        p = Path(f"D:/nine-deck-baseline-20261007/campaign/hot/qual/desktop-c16-w{workers}/report.json")
        r = load(p)
        assert r["complete"] and r["completed_updates"] == 1
        qualifications.append({"host": "desktop", "workers": workers, "cpu_affinity": r["placement"]["cpu_affinity"],
                               "seconds": r["seconds"], "fingerprint_sha256": hashlib.sha256(json.dumps(r["fingerprint"], sort_keys=True).encode()).hexdigest(), "source": pin(p)})
    remote = load(SCRATCH/"haley-training-timings.json")
    for item in remote["qualifications"]:
        r = item["report"]
        qualifications.append({"host": "computehost", "workers": r["placement"]["workers"],
                               "cpu_affinity": r["placement"]["cpu_affinity"], "seconds": r["seconds"], "source": item["pin"]})
    context["qualifications"] = qualifications
    (ROOT/"context.json").write_text(json.dumps(context, indent=2)+"\n", encoding="utf-8", newline="\n")
    print(json.dumps({"calendar": context["calendar"], "run_timing": timing, "archive_subset": len(archives),
                      "archive_seconds": sum(a["archive_seconds"] for a in archives),
                      "post_seconds_same_subset": sum(a["post_execution_seconds"] for a in archives)}, indent=2))


if __name__ == "__main__":
    main()
