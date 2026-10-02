"""Count complete native public-input checks without interpreting strength."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root",type=Path,required=True)
    parser.add_argument("--baseline-root",type=Path,required=True)
    parser.add_argument("--test-binary",type=Path,required=True)
    args = parser.parse_args()
    reports = []
    for label,seat in [("seat0",0),("seat1",1),("seat0-replay",0)]:
        path = args.root / f"match-{label}.json"
        report = json.loads(path.read_bytes())
        baseline_path = args.baseline_root / f"seat{seat}.json"
        baseline = json.loads(baseline_path.read_bytes())
        assert report["status"] == "ENGINEERING-PASS"
        assert report["baseline_sha256"] == sha(baseline_path)
        assert report["request_sha256"] == sha(args.baseline_root/f"request-seat{seat}.json")
        for key in ["actual_base_models","outcome","games","seed_resets","decisions"]:
            assert report[key] == baseline[key], (label,key)
        assert all(game["winner"] in [0,1] for game in report["games"])
        assert len(report["games"]) == len(report["seed_resets"]) == 2
        log = (args.root/f"match-{label}.log").read_text()
        seconds = re.search(r"1 passed; 0 failed;.*?finished in ([0-9.]+)s",log)
        assert seconds, label
        reports.append(dict(label=label,sha256=sha(path),games=len(report["games"]),
            decisions=len(report["decisions"]),test_seconds=float(seconds[1])))
    assert (args.root/"match-seat0.json").read_bytes() == (args.root/"match-seat0-replay.json").read_bytes()
    parity = json.loads((args.root/"native-parity-final.json").read_bytes())
    assert parity["status"] == "NATIVE-FORWARD-ENGINEERING-PASS"
    assert parity["fixture_sha256"] == sha(args.root/"python-probes.json")
    assert parity["zero_projection_native_scores_bit_exact"]
    assert all(v["hidden_pair_scores_bit_exact"] and v["samples"] == 12 for v in parity["variants"])
    for name,count,ignored in [("legacy-net-regression",8,0),("legacy-policy-regression",14,1),("native-parity-final",2,0)]:
        assert f"{count} passed; 0 failed; {ignored} ignored" in (args.root/f"{name}.log").read_text()
    result = dict(status="NATIVE-SCORER-AND-ZERO-ROLLOUT-ENGINEERING-PASS",unique_matches=2,
        unique_games=4,unique_decisions=sum(r["decisions"] for r in reports[:2]),
        match_executions=3,executed_games=6,match_test_seconds=sum(r["test_seconds"] for r in reports),
        exact_fresh_process_replay=True,matches=reports,parity=parity,
        regression_tests_passed=22,regression_tests_preexisting_ignored=1,
        test_binary_sha256=sha(args.test_binary),test_binary=str(args.test_binary),
        source_commit=subprocess.check_output(["git","rev-parse","HEAD"],text=True).strip(),
        rustc=subprocess.check_output(["rustc","--version"],text=True).strip(),
        cargo=subprocess.check_output(["cargo","--version"],text=True).strip(),
        non_claim="Debug engineering checks. No native optimizer/CUDA training, nonzero full-match qualification, promoted model, broad coverage or human-strength result.")
    with (args.root/"completion.json").open("x") as handle: json.dump(result,handle,indent=2)
    print(json.dumps({key:result[key] for key in ["status","unique_matches","unique_games","unique_decisions","match_test_seconds","exact_fresh_process_replay"]},indent=2))

if __name__ == "__main__": main()
