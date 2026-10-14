"""One CPU lease using the existing resident and external guards.

No provider mutation without create --execute. Credentials stay in memory.
Launch the independent guardian through Task Scheduler, outside Codex.
"""
from __future__ import annotations

import argparse
import base64
import copy
import ipaddress
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time

REPO = Path(__file__).resolve().parents[2]
HELPERS = REPO / "python/tools/phase1_cloud"
sys.path.insert(0, str(HELPERS))
sys.path.insert(0, str(REPO / "python/tools"))
import host_slots_v1
import external_guard
import lease_guard
import prepare_lease
from common import read, write
from runtime_observation import IMAGE_REFERENCE

KEY_PLACEHOLDER = "__INJECT_AT_POST_DO_NOT_SAVE__"


def prepare(root, name, public_key, region, now=None, *, deadline_epoch=None,
            prior_conservative_usd=0):
    now = time.time() if now is None else now
    if (isinstance(prior_conservative_usd, bool)
            or not isinstance(prior_conservative_usd, (int, float))
            or not math.isfinite(prior_conservative_usd)
            or not 0 <= prior_conservative_usd < 5):
        raise ValueError("prior conservative spend must be finite and below the original $5 cap")
    if prior_conservative_usd and deadline_epoch is None:
        raise ValueError("replacement preparation requires the original absolute deadline")
    deadline = now + 4 * 3600 if deadline_epoch is None else deadline_epoch
    if (isinstance(deadline, bool) or not isinstance(deadline, (int, float))
            or not math.isfinite(deadline) or not 600 < deadline - now <= 4 * 3600):
        raise ValueError("deadline must leave recovery time and cannot grant over four hours")
    if not re.fullmatch(r"search-sharing-allocation-20261013-[a-z0-9-]+", name):
        raise ValueError("use a unique experiment lane Pod name")
    parts = public_key.read_text().strip().split()
    if len(parts) < 2 or parts[0] != "ssh-ed25519":
        raise ValueError("existing ed25519 public key required")
    if root.exists():
        raise ValueError("fresh control directory required")
    lease = {
        "schema": "phase1-cloud-lease/v1", "name": name,
        "created_epoch": now, "deadline_epoch": deadline,
        "increment_cap_usd": 5 - prior_conservative_usd, "total_cap_usd": 5,
        "prior_conservative_usd": prior_conservative_usd, "rate_ceiling_usd_hour": 1.00,
        "storage_usd_hour": 100 * 0.10 / 720,
        "postrun_storage_reserve_usd": 0, "recovery_reserve_usd": 0.05,
        "funded_balance_usd": 5, "safety_multiplier": 1.2,
        "startup_idle_seconds": 3600, "work_idle_seconds": 300,
        "recovery_seconds": 600, "poll_seconds": 15,
        "network_volume_id": None,
    }
    projection = lease_guard.validate(lease)
    root.mkdir(parents=True)
    state = "/run/phase1/" + name
    blobs = {n: base64.b64encode((HELPERS / (n + ".py")).read_bytes()).decode()
             for n in ("common", "lease_guard")}
    guard_program = (
        "import base64,json,os,sys,types\nBLOBS=" + repr(blobs) + "\n"
        "for name in ('common','lease_guard'):\n"
        " module=types.ModuleType(name);sys.modules[name]=module\n"
        " exec(compile(base64.b64decode(BLOBS[name]),'<embedded-'+name+'>','exec'),module.__dict__)\n"
        "guard=sys.modules['lease_guard']\nlease=" + repr(lease) + "\n"
        "guard.run(lease," + repr(state) + ",os.environ.get('RUNPOD_POD_ID',''),"
        "expected_identity={'name':lease['name'],'network_volume_id':None},acquire_lock=True)\n"
    )
    bootstrap = "GUARD=" + repr(guard_program) + "\n" + r'''
import os,pathlib,subprocess,time
guard=subprocess.Popen(['python3','-B','-c',GUARD],start_new_session=True)
env=dict(os.environ);env.pop('RUNPOD_API_KEY',None)
env['DEBIAN_FRONTEND']='noninteractive'
try:
 for command in (['apt-get','update'],['apt-get','install','-y','--no-install-recommends','openssh-server','procps','util-linux','build-essential','curl','ca-certificates','pkg-config','rsync']):
  subprocess.run(command,env=env,check=True,timeout=600)
  if guard.poll() is not None: raise RuntimeError('resident guard exited')
 ssh=pathlib.Path('/root/.ssh');ssh.mkdir(exist_ok=True);ssh.chmod(0o700)
 (ssh/'authorized_keys').write_text(env['SSH_PUBLIC_KEY']+'\n')
 (ssh/'authorized_keys').chmod(0o600)
 pathlib.Path('/run/sshd').mkdir(exist_ok=True)
 subprocess.run(['ssh-keygen','-A'],env=env,check=True,timeout=30,stdout=subprocess.DEVNULL)
 pathlib.Path('/etc/ssh/sshd_config.d/experiment.conf').write_text('PermitRootLogin prohibit-password\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n')
 subprocess.Popen(['/usr/sbin/sshd','-D','-e'],env=env)
except Exception as e:
 print('Bootstrap unavailable: '+type(e).__name__,flush=True)
while guard.poll() is None: time.sleep(15)
raise SystemExit(guard.returncode)
'''
    body = {
        "name": name, "cloudType": "SECURE", "computeType": "CPU",
        "cpuFlavorIds": ["cpu3c"], "cpuFlavorPriority": "custom", "vcpuCount": 32,
        "containerDiskInGb": 100, "volumeInGb": 0,
        "imageName": IMAGE_REFERENCE, "interruptible": False,
        "supportPublicIp": True, "ports": ["22/tcp"],
        "dataCenterIds": [region], "dataCenterPriority": "custom",
        "dockerEntrypoint": ["python3", "-c", bootstrap], "dockerStartCmd": [],
        "env": {"RUNPOD_API_KEY": KEY_PLACEHOLDER,
                "SSH_PUBLIC_KEY": "__INJECT_EXISTING_PUBLIC_KEY_AT_POST__", "CUDA_VISIBLE_DEVICES": ""},
    }
    write(root / "lease/lease.json", lease)
    write(root / "lease/create-request-template.json", body)
    metadata = {"name": name, "public_key_path": str(public_key.resolve()),
                "resident_state": state, "remote_output": "/workspace/search-sharing-allocation/run",
                "projected_conservative_usd": projection,
                "projected_cumulative_conservative_usd": prior_conservative_usd + projection,
                "allocated": False}
    write(root / "preparation.json", metadata)
    return metadata


def matching_pods(key, name):
    matches = [p for p in external_guard.list_pods(key) if p.get("name") == name]
    if len(matches) > 1:
        raise ValueError("ambiguous exact-name ownership; no create or delete permitted")
    return matches


def safe_pod(pod, name):
    if pod.get("name") != name or not re.fullmatch(r"[A-Za-z0-9]+", str(pod.get("id", ""))):
        raise ValueError("provider Pod identity does not match the prepared name")
    return {"id": pod["id"], "name": name, "desired_status": pod.get("desiredStatus"),
            "cost_per_hour_usd": pod.get("costPerHr"), "data_center_id": pod.get("dataCenterId"),
            "cpu_flavor": pod.get("cpuFlavorId"), "vcpu_count": pod.get("vcpuCount"),
            "memory_gb": pod.get("memoryInGb"), "container_disk_gb": pod.get("containerDiskInGb")}


def create(root, execute):
    if not execute:
        raise ValueError("create requires --execute after the owner has spending authority")
    lease = read(root / "lease/lease.json")
    lease_guard.validate(lease)
    ready = read(root / "guardian-ready.json")
    if ready["name"] != lease["name"] or time.time() - ready["epoch"] > 300:
        raise ValueError("fresh independently launched guardian receipt required")
    if host_slots_v1.process_state(ready["pid"], ready["creation_time"]) != "alive":
        raise ValueError("guardian process identity is not currently alive")
    if time.time() > lease["created_epoch"] + 300:
        raise ValueError("prepare a fresh allocation timestamp without extending the approved deadline")
    key = external_guard.api_key()
    matches = matching_pods(key, lease["name"])
    if matches:
        result = safe_pod(matches[0], lease["name"])
        result["reconciled_exact_name"] = True
    else:
        body = copy.deepcopy(read(root / "lease/create-request-template.json"))
        metadata = read(root / "preparation.json")
        body["env"]["RUNPOD_API_KEY"] = key
        body["env"]["SSH_PUBLIC_KEY"] = Path(metadata["public_key_path"]).read_text().strip()
        try:
            result = safe_pod(prepare_lease.Provider(key).create(body), lease["name"])
        except (RuntimeError, ValueError, KeyError, TypeError):
            matches = matching_pods(key, lease["name"])
            if not matches:
                failure = root / "lease/create-failure.json"
                if failure.exists():
                    failure = root / ("lease/create-failure-" + str(time.time_ns()) + ".json")
                write(failure, {"epoch": time.time(),
                      "exact_name": lease["name"], "observed_matches": 0})
                raise RuntimeError("create outcome unavailable; exact-name census found no Pod; retry only this same prepared lease") from None
            result = safe_pod(matches[0], lease["name"])
            result["reconciled_exact_name"] = True
    write(root / "lease/created-pod.json", result, replace=True)
    if result["cost_per_hour_usd"] is not None and float(result["cost_per_hour_usd"]) > lease["rate_ceiling_usd_hour"]:
        write(root / "release-authorized.json", {"epoch": time.time(), "reason": "observed price exceeds ceiling"}, replace=True)
        raise ValueError("observed pod price exceeds the bounded lease; guardian release requested")
    return result


def preserve(root, pod_id):
    """Copy available outputs before deletion, with a bounded recovery window."""
    connection_path = root / "connection.json"
    if not connection_path.exists():
        return {"copied": False, "reason": "SSH connection not yet observed"}
    connection = read(connection_path)
    if connection.get("pod_id") != pod_id:
        raise ValueError("preservation connection belongs to another Pod")
    host = str(ipaddress.ip_address(connection["host"]))
    port = int(connection["port"])
    if not 1 <= port <= 65535:
        raise ValueError("invalid SSH port")
    remote = connection.get("remote_output", "/workspace/search-sharing-allocation/run")
    if not re.fullmatch(r"/workspace/[A-Za-z0-9_./-]+", remote) or ".." in remote:
        raise ValueError("output path must stay in the owned workspace")
    destination = root / "preserved-cloud-output"
    destination.mkdir(exist_ok=True)
    command = ["scp", "-r", "-i", connection["private_key_path"], "-P", str(port),
               "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new",
               "-o", "ConnectTimeout=15", f"root@{host}:{remote}", str(destination)]
    try:
        result = subprocess.run(command, timeout=300, capture_output=True)
        return {"copied": result.returncode == 0, "exit_code": result.returncode,
                "destination": str(destination)}
    except (OSError, subprocess.TimeoutExpired) as e:
        return {"copied": False, "error_type": type(e).__name__, "destination": str(destination)}


def guardian(root):
    lease = read(root / "lease/lease.json")
    key = external_guard.api_key()

    class PreservingProvider(lease_guard.Provider):
        def call(self, method):
            if method == "DELETE":
                pod = super().call("GET")
                if pod is None:
                    return None
                lease_guard.verify_pod_identity(pod, lease, self.pod_id)
                try:
                    receipt = preserve(root, self.pod_id)
                except Exception as error:
                    receipt = {"copied": False, "error_type": type(error).__name__}
                lease_guard.best_effort_receipt(root / "preservation.json", {"pod_id": self.pod_id,
                      "epoch": time.time(), **receipt})
            return super().call(method)

    pid, creation = host_slots_v1.self_identity()
    write(root / "guardian-ready.json", {"name": lease["name"], "pid": pid, "creation_time": creation,
          "epoch": time.time(), "deadline_epoch": lease["deadline_epoch"]}, replace=True)
    # Receipt failure must not prevent a deadline or completion DELETE.
    external_guard.write = lambda path, value, **kwargs: lease_guard.best_effort_receipt(path, value)
    external_guard.run(root, key, provider=PreservingProvider)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    p = sub.add_parser("prepare")
    p.add_argument("--control", type=Path, required=True)
    p.add_argument("--name", required=True)
    p.add_argument("--public-key", type=Path, default=Path("C:/Users/Jack/.ssh/id_ed25519.pub"))
    p.add_argument("--region", default="US-MD-1")
    p.add_argument("--deadline-epoch", type=float, help="original absolute deadline for replacement proposals")
    p.add_argument("--prior-conservative-usd", type=float, default=0)
    for mode in ("guardian", "create", "release", "status"):
        p = sub.add_parser(mode)
        p.add_argument("--control", type=Path, required=True)
        if mode == "create":
            p.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    if args.mode == "prepare":
        print(json.dumps(prepare(args.control, args.name, args.public_key, args.region,
                                 deadline_epoch=args.deadline_epoch,
                                 prior_conservative_usd=args.prior_conservative_usd)))
    elif args.mode == "create":
        print(json.dumps(create(args.control, args.execute)))
    elif args.mode == "guardian":
        guardian(args.control)
    elif args.mode == "status":
        lease = read(args.control / "lease/lease.json")
        matches = matching_pods(external_guard.api_key(), lease["name"])
        print(json.dumps({"name": lease["name"], "matching_pods": [safe_pod(p, lease["name"]) for p in matches]}))
    else:
        write(args.control / "release-authorized.json", {"epoch": time.time()}, replace=True)


if __name__ == "__main__":
    main()
