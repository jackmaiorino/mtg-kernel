"""One CPU lease using the existing resident and external guards.

No provider mutation without create --execute. Credentials stay in memory.
Launch the independent guardian through Task Scheduler, outside Codex.

Spend safety (mtg-kernel-collab#335):
- create holds an exclusive one-shot lock (lease/create.lock, never removed):
  one POST per prepared lease, a concurrent or repeated create refuses. It
  refuses on any earlier create receipt or any Pod already carrying the lease's
  exact name, requires a fresh account funding read (at most 120 s old at the
  POST, not under balance, the lease still fundable at the observed balance),
  records the returned identity before parsing any price, censuses after the
  POST and DELETEs duplicates. Every Pod ID it sees is retained under
  lease/known-pods/, so the guardian keeps deleting each until it alone is
  confirmed absent; post-creation diagnostics are best-effort. A missing, malformed or above-ceiling price
  (max of costPerHr and adjustedCostPerHr) requests release and DELETEs at once.
- an ambiguous POST (timeout, 5xx, transport error) with no Pod seen never
  resolves automatically: lease/create-ambiguous.json keeps the guardian
  deleting any Pod with the lease's name until an operator runs
  resolve-ambiguous. Only an explicit HTTP 4xx with an empty census is a
  definite failure.
- the guardian DELETEs without waiting on preservation: output copying runs
  at most once, only on a controller release or startup timeout, and only
  inside the lease time left. It reads the Pod-side resident state
  (worker-started.json or progress.json) over SSH, so productive work is not
  ended by the startup timeout.
- release is complete only after two absent observations at least 30 s apart
  (the guardian's own completion plus verify-absent); verify-absent's exit
  status gates task removal.

Task Scheduler procedure: register the guardian with no execution limit
(-ExecutionTimeLimit ([TimeSpan]::Zero)), restart on failure, a 5 minute
repetition with an empty duration and -MultipleInstances IgnoreNew; register a
watchdog task running `guardian --once` every 5 minutes. Remove both tasks only
after `verify-absent` exits 0; otherwise leave them armed.
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
FUNDING_MAX_AGE_SECONDS = 120
MIN_RELEASE_SECONDS = 120   # a creator's own release loop always gets time for two spaced absences
ABSENT_SPACING_SECONDS = 30
CREATE_RECEIPTS = ("lease/create-attempt.json", "lease/created-pod.json", "lease/create-failure.json",
                   "lease/create-ambiguous.json")


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


def lease_client(root, lease, key, provider=None, pods=None, clock=None, sleep=None):
    """The lease's provider boundary (external_guard.LeaseClient), always entered with `with`: every
    census, GET, DELETE and POST retains each owned Pod ID it sees under lease/known-pods/ before
    returning, and leaving the block resolves any ID that is neither durable nor verified absent, or
    fails loudly (ALERT-pod-not-released.json and RuntimeError)."""
    return external_guard.LeaseClient(Path(root) / external_guard.KNOWN_DIR, lease["name"], key, provider, pods,
                                      clock, sleep, Path(root) / "ALERT-pod-not-released.json",
                                      lease_deadline=lease["deadline_epoch"],
                                      min_release_seconds=MIN_RELEASE_SECONDS)


def best_effort(path, value):
    """Post-creation diagnostics never gate a DELETE."""
    try:
        write(path, value, replace=True)
        return True
    except Exception:  # noqa: BLE001
        return False


def finite_or_none(value):
    if isinstance(value, bool):
        return None
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    return number if math.isfinite(number) else None


def pod_identity(pod, name):
    """Identity only (id, name): never fails on pricing fields. The ID is validated as a raw string,
    never coerced, so JSON null or true can never become a Pod ID (I1)."""
    if not isinstance(pod, dict) or pod.get("name") != name or not external_guard.valid_id(pod.get("id")):
        raise ValueError("provider Pod identity does not match the prepared name")
    return {"id": pod["id"], "name": name}


def safe_pod(pod, name):
    """Identity plus sanitized detail; non-finite numbers become None so receipts always serialize."""
    out = pod_identity(pod, name)
    out.update({"desired_status": pod.get("desiredStatus") if isinstance(pod.get("desiredStatus"), str) else None,
                "cost_per_hour_usd": finite_or_none(pod.get("costPerHr")),
                "adjusted_cost_per_hour_usd": finite_or_none(pod.get("adjustedCostPerHr")),
                "data_center_id": pod.get("dataCenterId") if isinstance(pod.get("dataCenterId"), str) else None,
                "cpu_flavor": pod.get("cpuFlavorId") if isinstance(pod.get("cpuFlavorId"), str) else None,
                "vcpu_count": pod.get("vcpuCount") if type(pod.get("vcpuCount")) is int else None,
                "memory_gb": finite_or_none(pod.get("memoryInGb")),
                "container_disk_gb": finite_or_none(pod.get("containerDiskInGb"))})
    return out


def live_funds(key):
    """Fresh account read through the resident guard's read-only GraphQL query; raises on any error.
    It carries no Pod ID, so it is outside the Pod provider boundary (allowlisted in its test)."""
    return lease_guard.Provider(key, "funding").funds()


def create(root, execute, *, key=None, pods=None, poster=None, funds=None, provider=None, clock=time.time,
           sleep=time.sleep):
    """One POST per prepared lease, ever. Every refusal happens before the POST.

    Every census and the POST go through a LeaseClient, which retains each owned Pod ID before this code
    sees it (I1, I4); every exit, refusals included, resolves IDs that were not handed off (I4)."""
    if not execute:
        raise ValueError("create requires --execute after the owner has spending authority")
    root = Path(root)
    lease = read(root / "lease/lease.json")
    lease_guard.validate(lease)
    try:   # exclusive and never removed: a second or concurrent create of this lease always refuses
        fd = os.open(root / "lease/create.lock", os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        raise ValueError("create already attempted or in progress for this lease (one POST per prepared lease)") from None
    try:
        os.write(fd, json.dumps({"pid": os.getpid(), "epoch": clock()}).encode())
    finally:
        os.close(fd)
    for receipt in CREATE_RECEIPTS:
        if (root / receipt).exists():
            raise ValueError("an earlier create receipt exists: " + receipt)
    ready = read(root / "guardian-ready.json")
    if ready["name"] != lease["name"] or clock() - ready["epoch"] > 300:
        raise ValueError("fresh independently launched guardian receipt required")
    if host_slots_v1.process_state(ready["pid"], ready["creation_time"]) != "alive":
        raise ValueError("guardian process identity is not currently alive")
    if clock() > lease["created_epoch"] + 300:
        raise ValueError("prepare a fresh allocation timestamp without extending the approved deadline")
    key = key or external_guard.api_key()
    with lease_client(root, lease, key, provider, pods, clock, sleep) as client:

        def finish(also=()):
            """I4 at every exit: IDs not handed off are released here, or the create fails loudly."""
            return external_guard.resolve_or_alert(client, max(lease["deadline_epoch"], clock() + MIN_RELEASE_SECONDS),
                                                   root / "ALERT-pod-not-released.json", sleep, also)
        try:   # fresh funding through the API, at most FUNDING_MAX_AGE_SECONDS old at the POST
            observed = (funds or live_funds)(key)
            observed_epoch = clock()
            balance = float(observed["balance_usd"])
            under = observed["under_balance"]
        except Exception as error:  # noqa: BLE001
            raise ValueError("funding service unavailable: " + type(error).__name__) from None
        if under is not False or not math.isfinite(balance):
            raise ValueError("account under balance or balance unavailable")
        lease_guard.validate(dict(lease, funded_balance_usd=min(balance, lease["funded_balance_usd"])))
        write(root / "lease/funding-at-create.json", {"epoch": observed_epoch, "balance_usd": balance,
              "under_balance": under, "account_spend_usd_hour": finite_or_none(observed.get("account_spend_usd_hour"))})
        _, existing, _, malformed = client.census()   # each existing owned ID is retained before the refusal
        if existing or malformed:
            finish()
            raise ValueError("a Pod with this lease's name already exists: " + ",".join(existing or ["(malformed entry)"]))
        if clock() - observed_epoch > FUNDING_MAX_AGE_SECONDS:
            raise ValueError("funding observation older than 120 s at the POST")
        body = copy.deepcopy(read(root / "lease/create-request-template.json"))
        metadata = read(root / "preparation.json")
        body["env"]["RUNPOD_API_KEY"] = key
        body["env"]["SSH_PUBLIC_KEY"] = Path(metadata["public_key_path"]).read_text().strip()
        write(root / "lease/create-attempt.json", {"epoch": clock(), "exact_name": lease["name"]})
        # ---- the POST: the raw returned ID is retained by the client whatever its name (I1) ----
        returned = None; post_error = None
        try:
            returned = client.post(body, poster=poster)   # None: the default POST client, inside the boundary
        except Exception as error:  # noqa: BLE001 - outcome decided below; only an explicit 4xx is definite
            post_error = str(error) if isinstance(error, RuntimeError) else type(error).__name__
        del body
        post_id = returned.get("id") if isinstance(returned, dict) and external_guard.valid_id(returned.get("id")) else None
        census_ok = False; malformed = False; owned = []; entries = {}
        try:
            census, owned, _, malformed = client.census()
            entries = {e["id"]: e for e in census if isinstance(e, dict) and e.get("name") == lease["name"]
                       and external_guard.valid_id(e.get("id"))}
            census_ok = True
        except Exception as e:  # noqa: BLE001 - a failed census is never evidence of absence
            best_effort(root / "lease/post-census-error.json", {"epoch": clock(), "error_type": type(e).__name__})
        recorded = post_id; source = returned if post_id else None; reconciled = False
        if recorded is None and owned:
            recorded = sorted(owned)[0]; source = entries.get(recorded); reconciled = True
        if recorded:
            client.retain(recorded, "receipt")
            best_effort(root / "lease/created-pod.json", dict({"id": recorded, "name": lease["name"]},
                        **({"reconciled_exact_name": True} if reconciled else {})))
        released = finish()   # I4: IDs not handed off are deleted and verified absent here, or this fails loudly
        if recorded in released:
            raise RuntimeError("create handoff failed; the Pod was deleted and verified absent")
        extras = [p for p in owned if p != recorded and p not in released]
        deleted = []
        for pod_id in extras:   # duplicates are never kept; each is handed off, so the guardian keeps deleting it
            try:
                client.delete(pod_id)
                deleted.append(pod_id)
            except Exception:  # noqa: BLE001
                pass
        if extras:
            best_effort(root / "lease/duplicates-deleted.json", {"epoch": clock(), "extras": extras, "deleted": deleted})
        if recorded is None:
            definite = (post_error is not None and re.fullmatch(r"provider HTTP 4\d\d", post_error) is not None
                        and census_ok and not malformed and not owned)
            if definite:
                best_effort(root / "lease/create-failure.json", {"epoch": clock(), "exact_name": lease["name"],
                            "provider_error": post_error, "census_ok": True})
                raise RuntimeError("create refused by the provider (" + post_error + "); no Pod exists")
            best_effort(root / "lease/create-ambiguous.json", {"epoch": clock(), "exact_name": lease["name"],
                        "post_error": post_error, "census_ok": census_ok and not malformed})
            raise RuntimeError("create outcome ambiguous; the guardian deletes any Pod with this lease's name "
                               "until an operator runs resolve-ambiguous")
        # name, price and allocation validated from the raw data before any priced receipt is written
        try:
            rate = lease_guard.pod_rate(source if isinstance(source, dict) else {})
            ok = (source.get("name") == lease["name"] and rate <= lease["rate_ceiling_usd_hour"]
                  and source.get("vcpuCount") in (None, lease.get("vcpu_count", 32)))
        except Exception:  # noqa: BLE001 - NaN, missing or malformed price
            ok = False
        if not ok:
            best_effort(root / "release-authorized.json", {"epoch": clock(), "reason": "name, price or allocation invalid"})
            try:
                client.delete(recorded)
            except Exception:  # noqa: BLE001 - the guardian keeps deleting
                pass
            raise ValueError("observed pod price or allocation invalid; DELETE requested, guardian continues")
        detail = safe_pod(source, lease["name"])
        best_effort(root / "lease/created-pod-detail.json", detail)
        return detail


def preserve(root, pod_id, timeout):
    """Copy available outputs before deletion, bounded by the caller's remaining-time budget."""
    connection_path = root / "connection.json"
    if not connection_path.exists() or timeout <= 0:
        return {"copied": False, "reason": "no connection or no time"}
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
        result = subprocess.run(command, timeout=timeout, capture_output=True)
        receipt = {"copied": result.returncode == 0, "exit_code": result.returncode}
    except (OSError, subprocess.TimeoutExpired) as e:
        receipt = {"copied": False, "error_type": type(e).__name__}
    lease_guard.best_effort_receipt(root / "preservation.json", {"pod_id": pod_id, "epoch": time.time(),
                                    "timeout_s": timeout, "destination": str(destination), **receipt})
    return receipt


def pod_marker_probe(root, run=None):
    """Guardian-side check over SSH that the Pod's resident state shows a started worker.

    Nothing is read when the probe is built. At call time, a missing preparation,
    an unpublished IP or SSH port, or any SSH error means "not ready" (False).
    Host and port come from the provider's own Pod record."""
    root = Path(root)

    def probe(pod):
        try:
            metadata = read(root / "preparation.json")
            state = metadata["resident_state"]
            if not re.fullmatch(r"/run/phase1/[A-Za-z0-9-]+", state):
                return False
            private_key = str(Path(metadata["public_key_path"]).with_suffix(""))
            host = str(ipaddress.ip_address(pod.get("publicIp")))
            port = int((pod.get("portMappings") or {}).get("22"))
        except Exception:  # noqa: BLE001 - not ready
            return False
        if not 1 <= port <= 65535:
            return False
        check = "test -f " + state + "/worker-started.json -o -f " + state + "/progress.json && echo STARTED"
        try:
            result = (run or subprocess.run)(["ssh", "-i", private_key, "-p", str(port), "-o", "BatchMode=yes",
                          "-o", "StrictHostKeyChecking=accept-new",
                          "-o", "UserKnownHostsFile=" + str(root / "known_hosts"),
                          "-o", "ConnectTimeout=10", "root@" + host, check],
                         timeout=20, capture_output=True, text=True)
        except Exception:  # noqa: BLE001 - not ready
            return False
        return result.returncode == 0 and "STARTED" in (result.stdout or "")
    return probe


def guardian(root, once=False, *, key=None, run_guard=None):
    """Only the key is required before the guard loop; every optional step is failure-tolerant.
    No persisted result (verify-absent.json included) ends it: the guard always checks the provider."""
    root = Path(root)
    key = key or external_guard.api_key()
    if not once:
        try:
            lease = read(root / "lease/lease.json")
            pid, creation = host_slots_v1.self_identity()
            write(root / "guardian-ready.json", {"name": lease["name"], "pid": pid, "creation_time": creation,
                  "epoch": time.time(), "deadline_epoch": lease["deadline_epoch"]}, replace=True)
        except Exception:  # noqa: BLE001 - create refuses without a fresh receipt; the guard still runs
            pass
    # external_guard wraps its own writes: a failed receipt never prevents a DELETE, and a failed
    # known-Pod write is a store failure that blocks completion (I2).
    (run_guard or external_guard.run)(root, key, once=once,
                                      preserve=None if once else (lambda pod_id, t: preserve(root, pod_id, t)),
                                      marker_probe=pod_marker_probe(root))


def resolve_ambiguous(root, checked_by, *, key=None, pods=None, provider=None, clock=time.time, sleep=time.sleep):
    """Operator-only resolution of a create that may have allocated an unreceipted Pod, after checking
    the RunPod console and API."""
    root = Path(root)
    if not external_guard.create_unresolved(root):
        raise ValueError("no unresolved ambiguous create for this lease")
    if not isinstance(checked_by, str) or len(checked_by.strip()) < 10:
        raise ValueError("state who checked the RunPod console/API and what was seen")
    lease = read(root / "lease/lease.json")
    key = key or external_guard.api_key()
    with lease_client(root, lease, key, provider, pods, clock, sleep) as client:
        _, visible, _, malformed = client.census()   # each visible owned ID is retained first
        external_guard.resolve_or_alert(client, clock() + MIN_RELEASE_SECONDS, root / "ALERT-pod-not-released.json",
                                        sleep)
        if visible or malformed:
            raise ValueError("Pods with this lease's name are still visible; the guardian keeps deleting them: " +
                             ",".join(visible or ["(malformed entry)"]))
        write(root / external_guard.OPERATOR_RESOLUTION, {"epoch": clock(), "checked_by": checked_by.strip(),
              "census_owned_pods": 0, "rule": "operator checked the RunPod console and API; census empty"})
        return True


def verify_absent(root, *, key=None, pods=None, provider=None, clock=time.time, sleep=time.sleep):
    """Two observations >= 30 s apart, each a well-formed census (through the client, so a newly seen
    owned Pod is retained) and an exact-ID GET of every retained Pod ID; a failed or malformed read is
    never absence. An ID first seen during this check fails it (the next check covers it). Also requires
    the guardian's double-confirmed completion and refuses while a create is unresolved. IDs that could
    not be handed off are released here or reported with an ALERT. The result gates task removal."""
    root = Path(root)
    lease = read(root / "lease/lease.json")
    key = key or external_guard.api_key()
    with lease_client(root, lease, key, provider, pods, clock, sleep) as client:
        unresolved = external_guard.create_unresolved(root) or external_guard.ambiguous_unresolved(root)
        try:
            ids = set(external_guard.known_pods(root))   # strict: any unreadable record raises (I2)
            receipt_id, receipt_error = external_guard.receipt_state(root)
            if receipt_error:
                raise ValueError(receipt_error)
            if receipt_id:
                ids.add(receipt_id)
            for name in ("lease/create-ambiguous.json", "lease/create-pod-failure.json"):
                if (root / name).exists():
                    fallback = read(root / name).get("pod_id")
                    if fallback is not None:
                        ids.add(fallback)
            ids_ok = all(external_guard.valid_id(pod_id) for pod_id in ids)
        except Exception:  # noqa: BLE001 - unreadable retained IDs: never absence
            ids, ids_ok = set(), False
        initial = set(ids)
        observations = []
        for i in range(2):
            if i:
                sleep(ABSENT_SPACING_SECONDS)
            try:
                _, owned, listed, malformed = client.census()
                owned_count = None if malformed else len(owned)
            except Exception:  # noqa: BLE001 - a failed or malformed census is never absence
                owned_count, listed = None, set()
            ids |= set(client.retained)                   # newly seen owned IDs join the check
            present = {}
            for pod_id in sorted(ids) if ids_ok else []:
                if pod_id in listed:
                    present[pod_id] = True
                    continue
                try:
                    present[pod_id] = client.get(pod_id) is not None
                except Exception:  # noqa: BLE001 - including a malformed success response: never absence
                    present[pod_id] = None
            ids |= set(client.retained)
            observations.append({"epoch": clock(), "owned_pods": owned_count, "present_by_id": present})
        discovered = sorted(ids - initial)
        absent = ids_ok and not discovered and \
            all(o["owned_pods"] == 0 and all(v is False for v in o["present_by_id"].values()) for o in observations) and \
            all(set(o["present_by_id"]) == ids for o in observations) and \
            observations[1]["epoch"] - observations[0]["epoch"] >= ABSENT_SPACING_SECONDS
        completion = root / "external-guard-completion.json"
        try:
            guard_done = completion.exists() and read(completion).get("absent_confirmations") == 2
        except Exception:  # noqa: BLE001
            guard_done = False
        unpersisted = sorted(client.unpersisted)   # the block's exit releases these, or fails loudly (I4)
        released = absent and guard_done and not unresolved and not unpersisted
        record = {"observations": observations, "provider_absent_twice": absent, "guardian_completion": guard_done,
                  "create_unresolved": unresolved, "retained_ids": sorted(ids), "discovered_ids": discovered,
                  "unpersisted_ids": unpersisted, "released": released}
        best_effort(root / "verify-absent.json", record)
        if not released:
            best_effort(root / "ALERT-pod-not-released.json", {"epoch": clock(), **record})
        return released


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
    for mode in ("guardian", "create", "release", "status", "verify-absent", "resolve-ambiguous"):
        p = sub.add_parser(mode)
        p.add_argument("--control", type=Path, required=True)
        if mode == "create":
            p.add_argument("--execute", action="store_true")
        if mode == "guardian":
            p.add_argument("--once", action="store_true")
        if mode == "resolve-ambiguous":
            p.add_argument("--checked-by", required=True)
    args = parser.parse_args()
    if args.mode == "prepare":
        print(json.dumps(prepare(args.control, args.name, args.public_key, args.region,
                                 deadline_epoch=args.deadline_epoch,
                                 prior_conservative_usd=args.prior_conservative_usd)))
    elif args.mode == "create":
        print(json.dumps(create(args.control, args.execute)))
    elif args.mode == "guardian":
        guardian(args.control, once=args.once)
    elif args.mode == "status":
        lease = read(args.control / "lease/lease.json")
        # every exit, a formatting or printing error included, resolves unhanded IDs or fails loudly
        with lease_client(args.control, lease, external_guard.api_key()) as client:
            census, owned, _, malformed = client.census()   # retains every owned ID it reports
            print(json.dumps({"name": lease["name"], "malformed_entries": malformed, "matching_pods": [
                safe_pod(p, lease["name"]) for p in census
                if isinstance(p, dict) and p.get("name") == lease["name"] and p.get("id") in owned]}))
    elif args.mode == "resolve-ambiguous":
        print(json.dumps({"resolved": resolve_ambiguous(args.control, args.checked_by)}))
    elif args.mode == "verify-absent":
        released = verify_absent(args.control)
        print(json.dumps({"released_and_verified": released}))
        raise SystemExit(0 if released else 1)
    else:
        write(args.control / "release-authorized.json", {"epoch": time.time()}, replace=True)


if __name__ == "__main__":
    main()
