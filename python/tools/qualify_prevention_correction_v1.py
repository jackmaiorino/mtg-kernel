"""Fresh engineering replay after the public-observation correction, no training."""
import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import struct
import time

def load(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def write(path, payload):
    with path.open("x", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=pathlib.Path, required=True)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--previous", type=pathlib.Path, default=pathlib.Path("E:/mtg-postboard-campaign-20260920/gate-color-counterfactual-001"))
    args = parser.parse_args()
    args.root.mkdir(parents=True, exist_ok=False)
    binary = args.root / args.binary.name
    shutil.copy2(args.binary, binary)
    binary_bytes = binary.read_bytes()
    pe_offset = struct.unpack_from("<I", binary_bytes, 0x3c)[0]
    linker_version = struct.unpack_from("<BB", binary_bytes, pe_offset + 26)
    pins = []
    for seat in range(2):
        for kind in ["requests", "outputs"]:
            path = args.previous / kind / f"baseline-seat{seat}.json"
            pins.append(dict(path=str(path), sha256=sha(path)))
        shutil.copy2(args.previous / "requests" / f"baseline-seat{seat}.json", args.root / f"request-seat{seat}.json")
    write(args.root / "manifest.json", dict(schema="prevention-correction-engineering/v1", pins=pins,
        binary_sha256=sha(binary), source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        rustc=subprocess.check_output(["rustc", "-Vv"], text=True), cargo=subprocess.check_output(["cargo", "-V"], text=True),
        script_sha256=sha(pathlib.Path(__file__)), linker_version_from_pe="%d.%d" % linker_version,
        profile="debug", model_intervention="none; baseline mode, public observation correction only", max_worker_seconds=240,
        max_job_seconds=120, executions=3, gate="first elapsed times three below 240 seconds; exact replay; unshielded prefix exactly preserved",
        non_claim="Two development-exposed g115/A48 matches qualify execution and replay, not strength. No pooling with the prior runtime."))
    receipts, results = [], []
    try:
        for seat, replay in [(0,False), (0,True), (1,False)]:
            name = f"seat{seat}" + ("-replay" if replay else "")
            output = args.root / f"{name}.json"
            request = args.root / f"request-seat{seat}.json"
            assert load(request)["mode"] == "baseline"
            started = time.monotonic()
            with (args.root / f"{name}.stdout").open("wb") as stdout, (args.root / f"{name}.stderr").open("wb") as stderr:
                process = subprocess.Popen([str(binary), str(request), str(output)], stdout=stdout, stderr=stderr,
                    creationflags=getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0))
                timeout = False
                try:
                    code = process.wait(timeout=120)
                except subprocess.TimeoutExpired:
                    timeout = True
                    process.kill()
                    code = process.wait()
            elapsed = time.monotonic() - started
            receipt = dict(name=name, pid=process.pid, exit_code=code, timeout=timeout, seconds=elapsed,
                sha256=sha(output) if output.exists() else None)
            write(args.root / f"{name}.execution.json", receipt)
            receipts.append(receipt)
            assert code == 0 and not timeout, receipt
            assert sum(r["seconds"] for r in receipts) < 240
            current = load(output)
            assert all(d["raw_index"] == d["applied_index"] and d["raw_action"] == d["applied_action"] for d in current["decisions"])
            if len(receipts) == 1:
                write(args.root / "timing.json", dict(projected_seconds=elapsed*3, passed=elapsed*3 < 240))
                assert elapsed*3 < 240
            if replay:
                assert sha(output) == sha(args.root / "seat0.json")
                continue
            old = load(args.previous / "outputs" / f"baseline-seat{seat}.json")
            assert current["request"] == old["request"]
            assert current["actual_base_models"] == old["actual_base_models"]
            assert current["games"][0]["start"] == old["games"][0]["start"]
            assert current["games"][0]["environment_seed"] == old["games"][0]["environment_seed"]
            first_shield = next(index for index,d in enumerate(old["decisions"]) if d["applied_action"]["action_kind"] == "choose_effect_color" and d["applied_action"]["source"]["card_db_id"] == 88)
            assert current["decisions"][:first_shield+1] == old["decisions"][:first_shield+1], "unexpected change before shield resolution"
            assert current["decisions"][first_shield+1]["visible_sha256"] != old["decisions"][first_shield+1]["visible_sha256"], "shield still invisible in the completed match"
            results.append(dict(seat=seat, unchanged_prefix_decisions=first_shield+1,
                games=len(current["games"]), decisions=len(current["decisions"]),
                current_outcome=current["outcome"], old_outcome=old["outcome"]))
        for pin in pins:
            assert sha(pathlib.Path(pin["path"])) == pin["sha256"]
        write(args.root / "completion.json", dict(status="ENGINEERING-PASS", receipts=receipts, matches=results,
            worker_seconds=sum(r["seconds"] for r in receipts), unique_matches=2,
            non_claim="Corrected observation values changed deployment inputs. No learning or playing-strength conclusion."))
        print(json.dumps(dict(status="ENGINEERING-PASS", matches=results, worker_seconds=sum(r["seconds"] for r in receipts))))
    except BaseException as error:
        write(args.root / "failure.json", dict(status="INCOMPLETE", error=repr(error), receipts=receipts))
        raise

if __name__ == "__main__":
    main()
