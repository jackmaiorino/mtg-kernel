"""Measure real recovery of immutable full-panel files on a new placement.

These are copied engineering fixtures, never new scientific dispatch receipts.
The native dispatcher copies and verifies local trees and exports remote ZIPs;
the fixture exercises those same recovery operations without new simulation.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import time
import zipfile


def read(path): return json.loads(Path(path).read_bytes())
def pin(path):
    with Path(path).open('rb') as stream:
        return dict(path=str(path), sha256=hashlib.file_digest(stream, 'sha256').hexdigest())
def checked(item):
    assert pin(item['path'])['sha256'] == item['sha256'], item['path']
    return Path(item['path'])
def write(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
def fingerprint(hashes):
    return hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest()


def assignment(jobs, allocation):
    cycle = []
    for host in sorted(allocation):
        weight = allocation[host].get('job_weight', 1)
        assert type(weight) is int and 1 <= weight <= 8
        cycle.extend([host] * weight)
    assert cycle
    result = {host: [] for host in allocation}
    for index, job in enumerate(jobs):
        result[cycle[index % len(cycle)]].append(job['id'])
    assert all(result.values())
    return result


def host_hashes(source_hashes, job_ids, host):
    result = {}
    for name, digest in source_hashes.items():
        parts = Path(name).parts
        assert not Path(name).is_absolute() and '..' not in parts
        if parts[0] == 'jobs':
            if parts[1] not in job_ids: continue
        elif name == 'requests.zip' and host == 'desktop':
            continue  # The actual local dispatcher never creates this file.
        result[name] = digest
    assert {Path(n).parts[1] for n in result if n.startswith('jobs/')} == set(job_ids)
    return result


def stage(source, destination, hashes):
    source, destination = Path(source), Path(destination)
    destination.mkdir(parents=True)
    total = 0
    for name, digest in hashes.items():
        src, dst = source/name, destination/name
        assert src.resolve().is_relative_to(source.resolve())
        assert dst.resolve().is_relative_to(destination.resolve())
        assert pin(src)['sha256'] == digest
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        assert pin(dst)['sha256'] == digest
        total += dst.stat().st_size
    return dict(files=len(hashes), uncompressed_bytes=total, fingerprint=fingerprint(hashes))


def recover_local(source, destination, hashes):
    source, destination = Path(source), Path(destination)
    began = time.monotonic()
    shutil.copytree(source, destination)
    copied = time.monotonic()
    actual = {p.relative_to(source).as_posix(): pin(p)['sha256'] for p in source.rglob('*') if p.is_file()}
    assert actual == hashes
    assert {p.relative_to(destination).as_posix() for p in destination.rglob('*') if p.is_file()} == set(hashes)
    for name, digest in actual.items():
        assert pin(destination/name)['sha256'] == digest
    ended = time.monotonic()
    return dict(seconds=ended-began, stage_seconds=dict(copy=copied-began, full_readback=ended-copied),
                files=len(hashes), uncompressed_bytes=sum((destination/n).stat().st_size for n in hashes),
                match_bytes=sum((destination/n).stat().st_size for n in hashes if '/outputs/match-' in n),
                recovered_directory=str(destination), hashes=actual, mismatches=0)


def export(source, archive_path, hashes):
    began = time.monotonic()
    source = Path(source)
    with zipfile.ZipFile(archive_path, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
        for name, digest in hashes.items():
            path = source/name
            assert pin(path)['sha256'] == digest
            archive.write(path, name)
        archive.writestr('export-manifest.json', json.dumps(hashes))
    return dict(archive=pin(archive_path), seconds=time.monotonic()-began, files=len(hashes))


def validate_calibration(calibration_pin, allocation, target_plan, binary):
    c = read(checked(calibration_pin))
    assert c['schema'] == 'placed-full-panel-recovery-fixture/v1'
    for item in c['dependencies']: checked(item)
    reference = read(checked(c['reference_plan']))
    source = read(checked(c['source_dispatch']))
    recovery = read(checked(c['source_recovery']))
    assert c['binary'] == binary == reference['binary']
    assert c['placements'] == {h: {k: v for k, v in a.items() if k not in ('workers', 'job_weight')} for h, a in allocation.items()}
    assert c['allocation_weights'] == {h: a.get('job_weight', 1) for h, a in allocation.items()}
    assert source['matches'] == reference['expected_matches'] >= target_plan['expected_matches']
    assert len(source['jobs']) == reference['expected_jobs'] >= target_plan['expected_jobs']
    assert {j['id'] for j in source['jobs']} == {j['id'] for j in reference['jobs']}
    assert all(j['recovery'] == c['source_recovery'] for j in source['jobs'])
    assert recovery['mismatches'] == 0 and recovery['files'] == len(recovery['hashes'])
    assert c['source_fingerprint'] == fingerprint(recovery['hashes'])
    game_cap = max(m['config']['max_physical_games'] for j in reference['jobs'] for m in j['command']['matches'])
    assert all(j['command']['capture_decisions'] is False and
               all(m['config']['max_physical_games'] <= game_cap for m in j['command']['matches'])
               for j in target_plan['jobs'])
    assigned = assignment(reference['jobs'], allocation)
    assert c['assigned_reference_jobs'] == assigned
    samples = c['samples']
    assert len(samples) >= 2 and len({s['path'] for s in samples}) == len(samples)
    times, match_totals, per_host = [], [], None
    for sample in samples:
        r = read(checked(sample))
        assert r['schema'] == 'placed-recovery-sample/v1' and r['complete']
        assert set(r['hosts']) == set(allocation) and r['mismatches'] == 0
        assert r['source_fingerprint'] == c['source_fingerprint']
        assert math.isfinite(r['seconds']) and r['seconds'] > 0
        accounted = total_matches = 0
        host_sizes = {}
        for host, evidence in r['hosts'].items():
            expected = host_hashes(recovery['hashes'], assigned[host], host)
            assert evidence['hashes'] == expected and evidence['mismatches'] == 0
            assert evidence['files'] == len(expected)
            assert evidence['stage_seconds'] and all(math.isfinite(v) and v >= 0 for v in evidence['stage_seconds'].values())
            assert sum(evidence['stage_seconds'].values()) <= evidence['seconds'] + .01
            recovered = Path(evidence['recovered_directory'])
            allowed = set(expected) | ({'export-manifest.json'} if host == 'computehost' else set())
            assert {p.relative_to(recovered).as_posix() for p in recovered.rglob('*') if p.is_file()} == allowed
            for name, digest in expected.items(): assert pin(recovered/name)['sha256'] == digest
            size = sum((recovered/n).stat().st_size for n in expected)
            match_bytes = sum((recovered/n).stat().st_size for n in expected if '/outputs/match-' in n)
            assert evidence['uncompressed_bytes'] == size and evidence['match_bytes'] == match_bytes
            if host == 'computehost':
                checked(evidence['archive'])
                assert read(recovered/'export-manifest.json') == expected
                with zipfile.ZipFile(evidence['archive']['path']) as archive:
                    assert set(archive.namelist()) == allowed
            total_matches += match_bytes
            host_sizes[host] = match_bytes
            accounted += evidence['seconds']
        # The production dispatcher recovers hosts sequentially in sorted order.
        assert accounted <= r['seconds'] + .01
        times.append(r['seconds']); match_totals.append(total_matches)
        assert per_host is None or per_host == host_sizes
        per_host = host_sizes
    assert len(set(match_totals)) == 1
    return samples, times, per_host


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('mode', choices=['stage', 'export'])
    parser.add_argument('source')
    parser.add_argument('destination')
    parser.add_argument('hashes')
    args = parser.parse_args()
    assert __debug__, 'validation must be enabled'
    function = stage if args.mode == 'stage' else export
    print(json.dumps(function(args.source, args.destination, read(args.hashes))))
