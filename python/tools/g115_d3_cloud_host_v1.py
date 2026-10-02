"""Compare actual cloud CPU, quota, memory and storage with qualification."""
import os
from pathlib import Path
import shutil

from g115_d3_payload_v1 import require

CGROUP_FILES = (
    '/proc/self/mountinfo', '/sys/fs/cgroup/cpu.max',
    '/sys/fs/cgroup/cpu/cpu.cfs_quota_us', '/sys/fs/cgroup/cpu/cpu.cfs_period_us',
    '/sys/fs/cgroup/cpu,cpuacct/cpu.cfs_quota_us', '/sys/fs/cgroup/cpu,cpuacct/cpu.cfs_period_us',
    '/sys/fs/cgroup/memory.max', '/sys/fs/cgroup/memory/memory.limit_in_bytes',
)


def observe():
    require(os.name == 'posix', 'Cloud resource observation requires Linux')
    host = dict(cpu_info=Path('/proc/cpuinfo').read_text(), affinity=sorted(os.sched_getaffinity(0)),
                memory=Path('/proc/meminfo').read_text(), disk=shutil.disk_usage('/workspace')._asdict())
    groups = {name: Path(name).read_text() for name in CGROUP_FILES if Path(name).exists()}
    return host, groups


def profile(host, groups):
    affinity = set(host['affinity'])
    models, cores, seen = set(), {}, set()
    for block in host['cpu_info'].split('\n\n'):
        fields = dict(line.split(':', 1) for line in block.splitlines() if ':' in line)
        fields = {k.strip(): v.strip() for k, v in fields.items()}
        cpu = int(fields.get('processor', '-1'))
        if cpu in affinity:
            require(all(k in fields for k in ('model name', 'physical id', 'core id')), 'CPU topology missing')
            models.add(fields['model name'])
            core = (fields['physical id'], fields['core id'])
            cores[core] = cores.get(core, 0) + 1
            seen.add(cpu)
    require(seen == affinity and affinity, 'Affinity CPU topology incomplete')
    capacity = float(len(affinity))
    if '/sys/fs/cgroup/cpu.max' in groups:
        quota, period = groups['/sys/fs/cgroup/cpu.max'].split()
        if quota != 'max':
            capacity = min(capacity, int(quota) / int(period))
    else:
        candidates = [p for p in ('/sys/fs/cgroup/cpu', '/sys/fs/cgroup/cpu,cpuacct')
                      if p + '/cpu.cfs_quota_us' in groups]
        require(bool(candidates), 'CPU quota observation missing')
        prefix = candidates[0]
        quota = int(groups[prefix + '/cpu.cfs_quota_us'])
        if quota != -1:
            capacity = min(capacity, quota / int(groups[prefix + '/cpu.cfs_period_us']))
    meminfo = dict(line.split(':', 1) for line in host['memory'].splitlines() if ':' in line)
    memory = int(meminfo['MemTotal'].split()[0]) * 1024
    limits = [groups[p].strip() for p in ('/sys/fs/cgroup/memory.max',
              '/sys/fs/cgroup/memory/memory.limit_in_bytes') if p in groups]
    require(bool(limits), 'Container memory limit observation missing')
    for limit in limits:
        if limit != 'max':
            memory = min(memory, int(limit))
    mounts = [line.split() for line in groups['/proc/self/mountinfo'].splitlines()]
    workspace = [line for line in mounts if len(line) > 5 and line[4] == '/workspace']
    require(len(workspace) == 1, 'Workspace filesystem observation missing')
    filesystem = workspace[0][workspace[0].index('-') + 1]
    return dict(cpu_models=sorted(models), logical_cpus=len(affinity), physical_cores=len(cores),
                threads_per_core=sorted(cores.values()), effective_cpu_capacity=capacity,
                memory_limit_bytes=memory, workspace_filesystem=filesystem)


def require_compatible(actual, qualified):
    for key in ('cpu_models', 'logical_cpus', 'physical_cores', 'threads_per_core', 'workspace_filesystem'):
        require(actual[key] == qualified[key], 'Requalify changed cloud hardware: ' + key)
    require(actual['effective_cpu_capacity'] >= qualified['effective_cpu_capacity'], 'Requalify reduced CPU quota')
    require(actual['memory_limit_bytes'] >= qualified['memory_limit_bytes'], 'Requalify reduced memory capacity')
