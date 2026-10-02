"""Small cross-device replay and timing qualification, never a campaign launcher."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time


def read(path):
    return json.loads(Path(path).read_bytes())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, data):
    with Path(path).open("x") as stream:
        json.dump(data, stream, indent=2, allow_nan=False)


def run(root, device, workers, label):
    plan = read(root/"device-qualification-plan.json")
    binary = root/"public_feature_training_v1.exe"
    config_path = root/"configs/structured.json"
    assert sha(binary) == plan["binary_sha256"]
    assert sha(config_path) == plan["config_sha256"]
    assert sha(__file__) == plan["runner_sha256"]
    config = read(config_path)
    assert config["projection_mode"] == "state_only" and config["inputs_enabled"]
    assert len(config["updates"]) == 4 and all(len(batch) == 2 for batch in config["updates"])
    assert device in [0, 1] and workers in [1, 2]
    assert label and all(c.isalnum() or c in "-_" for c in label)
    folder = root/label
    folder.mkdir()
    request = dict(config=config, output_directory=str(folder/"outputs"), resume=None, stop_after=4,
                   execution_gpu_ordinal=device, collector_workers=workers)
    write(folder/"request.json", request)
    before = time.monotonic()
    with (folder/"stdout").open("x") as stdout, (folder/"stderr").open("x") as stderr:
        child = subprocess.Popen([str(binary), str(folder/"request.json")], stdout=stdout, stderr=stderr,
                                 creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
        write(folder/"started.json", dict(pid=child.pid,device=device,workers=workers,unix_time=time.time()))
        timeout = False
        try:
            code = child.wait(timeout=180)
        except subprocess.TimeoutExpired:
            timeout = True
            child.kill()
            code = child.wait()
    execution = dict(exit_code=code,timeout=timeout,seconds=time.monotonic()-before,
                     binary_sha256=sha(binary),request_sha256=sha(folder/"request.json"))
    write(folder/"execution.json", execution)
    assert code == 0 and not timeout, execution
    completion = read(folder/"outputs/completion.json")
    assert completion["first_update"] == 0 and completion["next_update"] == 4
    assert completion["execution_gpu_ordinal"] == device and completion["collector_workers"] == workers
    assert len(completion["receipts"]) == 4
    outputs = {}
    for update, receipt in enumerate(completion["receipts"]):
        assert receipt["update"] == update and receipt["natural_games"] == 2 and receipt["episodes"] == 2
        assert receipt["execution_gpu_ordinal"] == device and receipt["collector_workers"] == workers
        for name in ["checkpoint.json", "optimizer.json", "episode-000.json", "episode-001.json"]:
            relative = f"{update:04}/{name}"
            outputs[relative] = sha(folder/"outputs"/relative)
    mismatches = [name for name, digest in outputs.items() if digest != plan["reference_outputs"][name]]
    report = dict(complete=True,byte_exact=not mismatches,mismatches=mismatches,outputs=outputs,
                  device=device,workers=workers,process_seconds=execution["seconds"],
                  update_seconds=sum(r["seconds"] for r in completion["receipts"]),
                  collection_seconds=sum(r["collection_seconds"] for r in completion["receipts"]),
                  non_claim="Four two-game updates only. Not representative production placement or strength evidence.")
    write(folder/"qualification.json",report)
    print(json.dumps({k:v for k,v in report.items() if k != "outputs"},indent=2))
    assert not mismatches, "cross-device output differs; diagnose before more execution"


if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--root",type=Path,required=True)
    parser.add_argument("--device",type=int,required=True)
    parser.add_argument("--workers",type=int,required=True)
    parser.add_argument("--label",required=True)
    args=parser.parse_args()
    run(args.root,args.device,args.workers,args.label)
