"""Prepare line (a) launch manifests from pinned inputs; never dispatches or reads outcomes.

Throughput mode: the bounded timing check of COMPUTE-POLICY item 2 for the
ordinary BO3 work class. 56 BO3 (2 engineering blocks x 2 members x 7 learner
decks x 2 seats) with V3 and untouched g115 as members and g115 as learner, on
engineering seed labels outside every frozen packet (the calibration and
screen seeds are never played before their measurements). The g115 and V3
leaves are relocated into SSD scratch and V3's transfer envelope is rebound to
the build commit by the unchanged g115_d3_payload_v1.prepare, after a scratch
manifest is written and under the e-io lock (storage ruling R3(b), (f)).
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import re
import shutil
import subprocess
import time

import g115_d3_payload_v1 as payload
import g115_line_a_guard_v1 as guard
import g115_line_a_manifest_v1 as manifest
import g115_line_a_windows_launch_v1 as launcher

ENGINEERING_PREFIX = 'g115-line-a-engineering-v1|'
OWNER = 'opus-line-a-launcher'
HERE = Path(__file__).resolve().parent
DOCUMENTS = ('g115_line_a_windows_launch_v1.py', 'g115_line_a_guard_v1.py', 'g115_line_a_manifest_v1.py',
             'g115_line_a_seeds_v1.py', 'g115_line_a_prepare_v1.py', 'g115_d3_payload_v1.py',
             'g115_d4_audit_storage_v1.py')


def engineering_seed(label):
    """Same construction as the v2 rule, under a distinct engineering prefix."""
    return int.from_bytes(hashlib.sha256((ENGINEERING_PREFIX + label).encode('ascii')).digest()[:8], 'big')


def representative_jobs(members, blocks):
    jobs = []
    for j in blocks:
        label = 'throughput/block/%02d' % j
        seed = engineering_seed(label)
        for member in members:
            for i, deck in enumerate(manifest.DECKS):
                for seat in manifest.SEATS:
                    jobs.append(dict(id='thr-b%02d-%s-d%d-s%d' % (j, member, i, seat), block=j, seed_label=label,
                                     seed=seed, member=member, learner_deck=deck,
                                     opponent_deck=manifest.opponent_deck(i, j), learner_seat=seat))
    return jobs


def ref(path):
    path = Path(path)
    return dict(path=str(path).replace('\\', '/'), sha256=guard.sha256_file(path))


def data_tree_sha256(repo):
    """Hash of the build worktree's data/ tree (relative paths and file hashes), bound into the evidence."""
    digest = hashlib.sha256()
    base = Path(repo) / 'data'
    for path in sorted(p for p in base.rglob('*') if p.is_file()):
        digest.update(path.relative_to(base).as_posix().encode() + b'\0' + guard.sha256_file(path).encode() + b'\n')
    return digest.hexdigest()


def leaf_sources(template):
    """Every file the payload step reads, with hash and bytes, for the scratch manifest."""
    refs = [template['source']['checkpoint'], template['source']['play_import'], manifest.V3_SOURCE['play_import']]
    for descriptor in (template['source']['play_import'], manifest.V3_SOURCE['play_import']):
        body = guard.read(guard.checked(descriptor))
        refs += [value for value in body.values() if isinstance(value, dict) and set(value) == {'path', 'sha256'}]
    return [dict(path=r['path'].replace('\\', '/'), sha256=r['sha256'], bytes=Path(r['path']).stat().st_size)
            for r in refs]


def prepare_throughput(root, build_receipt, build_repo, host='jack', worker_counts=(1, 8, 24), blocks=(0, 1),
                       job='g115-line-a-throughput-001'):
    build = guard.read(build_receipt)
    require = guard.require
    require(build['exit_code'] == 0 and build['clean'] is True, 'Engineering build did not complete cleanly')
    commit = build['commit']
    evaluator = build['binaries']['public_feature_evaluation_v1']
    guard.require_pinned(evaluator['pinned'], evaluator['sha256'])
    template = manifest.load_template()
    root = Path(root)
    require(guard.normalized(root).startswith(guard.normalized(guard.SCRATCH_PARENT) + job.lower() + '/'),
            'Preparation must live under D:/e-scratch/<job>/')
    root.mkdir(parents=True)
    sources = leaf_sources(template)
    scratch = dict(schema=guard.SCRATCH_SCHEMA, job=job, owner=OWNER, host=host,
                   root=guard.SCRATCH_PARENT + job + '/', sources=sources,
                   disposable=['preparation/payload/inputs/**', 'run/**/native/**'],
                   cap_bytes=2 * 10**9)
    launcher.write_json(root / 'scratch-manifest.json', scratch)
    guard.acquire_e_io(OWNER, 'line (a) throughput scratch fill', 600)
    try:
        bound = payload.prepare(dict(candidate=dict(kind='legacy', source=template['source'], v3_forced_actions=False),
                                     opponent=dict(kind='legacy', source=manifest.V3_SOURCE, v3_forced_actions=True,
                                                   v3_spell_target_reference_adapter=True)),
                                root / 'payload', str(root / 'payload').replace('\\', '/'), commit)
    finally:
        guard.release_e_io(OWNER)
    learner = bound['candidate']
    members = dict(v3=bound['opponent'], g115=learner)
    jobs = representative_jobs(('v3', 'g115'), blocks)
    documents = {name.replace('.py', ''): ref(HERE / name) for name in DOCUMENTS}
    documents['launcher'] = documents.pop('g115_line_a_windows_launch_v1')
    launch = dict(
        schema=launcher.LAUNCH_SCHEMA, mode='throughput', job=job, documents=documents,
        transport=dict(dispatcher=ref(HERE / 'g115_d3_windows_dispatch.ps1')),
        executable=dict(path=evaluator['pinned'], sha256=evaluator['sha256'], git_head=commit,
                        build_receipt=ref(build_receipt)),
        binding=dict(launcher_sha256=documents['launcher']['sha256'], executable_sha256=evaluator['sha256'],
                     data_tree_sha256=data_tree_sha256(build_repo), work_class='bo3-ordinary'),
        deck_packet=manifest.deck_packet(template), learner_source=learner, member_sources=members,
        payload=ref(root / 'payload' / 'sources.json'),
        throughput=dict(jobs=jobs, worker_counts=list(worker_counts), work_class='bo3-ordinary',
                        seed_rule='unsigned big-endian first 8 bytes of SHA256(ASCII("%s" + label))'
                                  % ENGINEERING_PREFIX,
                        cap_bytes=2 * 10**9, control_allowance_bytes=64 * 2**20,
                        reservation_bytes=32 * 2**20, job_timeout_seconds=3600),
        hosts={host: dict(volume='D:/')}, scratch_manifest=ref(root / 'scratch-manifest.json'),
        scope='engineering timing check; outcomes are never read; no frozen seed is played')
    require(len(jobs) <= launcher.THROUGHPUT_MAX_JOBS, 'Timing check too large')
    launcher.write_json(root / 'launch-manifest.json', launch)
    return launch


BUSY = re.compile(r'^(public_feature_evaluation_v1|trainer|mtg_kernel.*|cargo|rustc)\.exe$', re.I)
HALEYSPC = 'haley@100.71.75.65'


def jack_facts():
    """Jack's PC measured now: CPUs, memory, free disk, GPUs and competing native work."""
    import psutil
    gpus = subprocess.run(['nvidia-smi', '--query-gpu=index,name,uuid,memory.total,memory.used',
                           '--format=csv,noheader'], capture_output=True, text=True).stdout.strip().splitlines()
    competing = sorted({p.info['name'] for p in psutil.process_iter(['name']) if BUSY.match(p.info['name'] or '')})
    return dict(checked_unix=time.time(), logical_cpus=psutil.cpu_count(), memory_available_bytes=
                psutil.virtual_memory().available, d_free_bytes=shutil.disk_usage('D:/').free, gpus=gpus,
                competing=competing, eligible=not competing,
                reason='idle local host' if not competing else 'competing native work: ' + ', '.join(competing))


def haleyspc_facts():
    """Reachability and CPUs over SSH; the line (a) payload is not staged there (reported, not engineered)."""
    probe = subprocess.run(['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=8', HALEYSPC,
                            'echo %NUMBER_OF_PROCESSORS%'], capture_output=True, text=True, timeout=30)
    reachable = probe.returncode == 0
    return dict(checked_unix=time.time(), reachable=reachable,
                logical_cpus=int(probe.stdout.strip()) if reachable and probe.stdout.strip().isdigit() else None,
                eligible=False,
                reason='reachable over SSH, but the pinned evaluator, payload and launcher are not staged on its '
                       'mirrored paths; staging is remaining engineering' if reachable else 'SSH unreachable')


def runpod_facts():
    return dict(checked_unix=time.time(), eligible=False,
                reason='the WMI transport and this launcher are Windows-only; no Linux path exists for this '
                       'workload, and no paid allocation is authorized for line (a) (R12)')


def throughput_evidence(host_records, inventory, overhead_seconds, units, binding, work_class):
    """Assemble g115-line-a-throughput/v1: every measured allocation compared, fastest feasible selected."""
    hosts = {}
    for record in host_records:
        guard.require(record['work_class'] == work_class and record['binding'] == binding,
                      'Host evidence binds a different workload')
        hosts[record['host']] = dict(overhead_seconds=overhead_seconds[record['host']],
                                     phases=[{k: phase[k] for k in ('workers', 'seconds', 'completed', 'rows')}
                                             for phase in record['phases']])
    evidence = dict(schema=guard.THROUGHPUT_SCHEMA, binding=dict(binding, work_class=work_class),
                    inventory=inventory, hosts=hosts, placements=[], selected=None, units=units)
    allocations = [{host: phase['workers']} for host, item in hosts.items() for phase in item['phases']]
    if len(hosts) > 1:
        fastest = {host: max(item['phases'], key=guard.phase_rate)['workers'] for host, item in hosts.items()}
        for size in range(2, len(hosts) + 1):
            allocations += [{h: fastest[h] for h in names} for names in itertools.combinations(sorted(hosts), size)]
    for allocation in allocations:
        seconds = guard.projected_seconds(evidence, allocation, units)
        evidence['placements'].append(dict(id='+'.join('%s-%d' % item for item in sorted(allocation.items())),
                                           allocation=allocation, eligible=True, ineligibility_reason='',
                                           projected_seconds=seconds))
    evidence['selected'] = min(evidence['placements'], key=lambda p: (p['projected_seconds'], p['id']))['id']
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--build-receipt', type=Path, required=True)
    parser.add_argument('--build-repo', type=Path, required=True)
    parser.add_argument('--host', default='jack', choices=['jack', 'haleyspc'])
    args = parser.parse_args()
    launch = prepare_throughput(args.root, args.build_receipt, args.build_repo, args.host)
    print(json.dumps(dict(launch_manifest=str(args.root / 'launch-manifest.json'),
                          sha256=guard.sha256_file(args.root / 'launch-manifest.json'),
                          jobs=len(launch['throughput']['jobs']), worker_counts=launch['throughput']['worker_counts'],
                          seeds={j['seed_label']: j['seed'] for j in launch['throughput']['jobs']})))


if __name__ == '__main__':
    main()
