"""Configure only an owned Linux Pod's slots from observed affinity and quota."""
import argparse
import math
import os
from pathlib import Path
import platform
import re
import shutil

from launch import GIB, publish, require


def configure(root, workspace, out):
    require(platform.system() == "Linux", "owned Pod configuration requires Linux")
    affinity = sorted(os.sched_getaffinity(0))
    observed = {}
    capacity = float(len(affinity))
    v2 = Path("/sys/fs/cgroup/cpu.max")
    if v2.exists():
        observed[str(v2)] = v2.read_text().strip()
        quota, period = observed[str(v2)].split()
        if quota != "max":
            capacity = min(capacity, int(quota) / int(period))
    else:
        for folder in ("/sys/fs/cgroup/cpu", "/sys/fs/cgroup/cpu,cpuacct"):
            quota_path, period_path = Path(folder, "cpu.cfs_quota_us"), Path(folder, "cpu.cfs_period_us")
            if quota_path.exists() and period_path.exists():
                quota, period = int(quota_path.read_text()), int(period_path.read_text())
                observed.update({str(quota_path): str(quota), str(period_path): str(period)})
                if quota != -1:
                    capacity = min(capacity, quota / period)
                break
    require(observed and capacity >= 1, "CPU quota missing or below one useful core")
    cores = affinity[:math.floor(capacity)]
    require(shutil.disk_usage(workspace).free >= 60 * GIB + 20 * GIB,
            "Pod needs 60 GiB reserve plus 16 GiB build and 4 GiB run capacity")
    host = re.sub(r"[^A-Za-z0-9-]", "-", platform.node()).upper()
    directory = Path(root, host + ".slots")
    directory.mkdir(parents=True, exist_ok=True)
    config = {"cores": ",".join(map(str, cores)), "keep_free": 0, "priority": "below_normal"}
    target = directory / "config.json"
    require(not target.exists(), "preserve existing allocator configuration; inspect it explicitly")
    profile = {"host": platform.node().upper(), "affinity": affinity, "configured_cores": cores,
               "effective_cpu_capacity": capacity, "quota_observation": observed,
               "cpu_info": Path("/proc/cpuinfo").read_text(), "memory_info": Path("/proc/meminfo").read_text(),
               "mountinfo": Path("/proc/self/mountinfo").read_text(),
               "disk": shutil.disk_usage(workspace)._asdict(), "workspace": str(workspace)}
    for name in ("/sys/fs/cgroup/memory.max", "/sys/fs/cgroup/memory/memory.limit_in_bytes"):
        if Path(name).exists():
            profile.setdefault("memory_limit_observation", {})[name] = Path(name).read_text().strip()
    require(profile.get("memory_limit_observation"), "container memory limit observation missing")
    publish(out, profile)
    publish(target, config)
    return {"host": host, "configuration": str(target), "configured_cores": cores,
            "effective_cpu_capacity": capacity, "profile": str(out)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default="/var/lib/mtg-node/host-lock")
    parser.add_argument("--workspace", default="/workspace")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    print(configure(args.root, args.workspace, args.out))
