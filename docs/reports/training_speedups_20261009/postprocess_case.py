"""Inspect one full comparison case, or retain it after pinned independent recovery.

No remote execution, automatic recovery copying, retries, or training. Run the
same file for both variants. Inspection never prunes. Retention requires a
separately supplied SHA-pinned desktop E: copy verification receipt.
"""
from __future__ import annotations

import argparse
import ctypes
from datetime import datetime, timezone
import hashlib
import importlib
import json
import os
from pathlib import Path, PureWindowsPath
import platform
import shutil
import sys
import time
from types import SimpleNamespace


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path):
    path = Path(path)
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest}


def read_pinned(item):
    payload = Path(item['path']).read_bytes()
    require(hashlib.sha256(payload).hexdigest() == item['sha256'], 'changed pin: ' + item['path'])
    return json.loads(payload)


def save(path, document):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(document, stream, indent=2, allow_nan=False)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())
    return pin(path)


def checked_path(path):
    path = Path(path)
    require(path.is_absolute() and '..' not in path.parts, 'absolute non-traversing path required')
    for entry in (path, *path.parents):
        if entry.exists():
            require(not entry.is_symlink() and not entry.is_junction()
                    and not (getattr(entry.stat(follow_symlinks=False), 'st_file_attributes', 0) & 0x400),
                    'reparse/link path refused: ' + str(entry))
    return path.resolve()


def files_in_tree(root):
    root = checked_path(root)
    require(root.is_dir(), 'missing tree: ' + str(root))
    files = []
    for folder, directories, names in os.walk(root, followlinks=False):
        for name in directories + names:
            path = Path(folder) / name
            checked_path(path)
            if path.is_file():
                files.append(path)
    return sorted(files)


def physical_stats(root):
    require(os.name == 'nt', 'physical allocation measurement requires Windows')
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    function = kernel.GetCompressedFileSizeW
    function.argtypes = [ctypes.c_wchar_p, ctypes.POINTER(ctypes.c_ulong)]
    function.restype = ctypes.c_ulong
    logical = allocated = 0
    files = files_in_tree(root)
    for path in files:
        logical += path.stat().st_size
        high = ctypes.c_ulong()
        ctypes.set_last_error(0)
        low = function(str(path), ctypes.byref(high))
        if low == 0xffffffff and ctypes.get_last_error():
            raise ctypes.WinError(ctypes.get_last_error())
        allocated += low + (high.value << 32)
    return {'files': len(files), 'logical_bytes': logical, 'allocated_file_bytes': allocated,
            'method': 'GetCompressedFileSizeW', 'excludes_directory_and_volume_metadata': True}


def inputs(args):
    comparison = checked_path(args.comparison_root)
    request_path = checked_path(args.request)
    request_pin = pin(request_path)
    request = read_pinned(request_pin)
    dispatch_root = checked_path(request['root'])
    cold = checked_path(request['cold_root'])
    require(dispatch_root.parent == comparison / 'measure' / 'hot', 'dispatch outside owned comparison hot root')
    require(cold.parent == comparison / 'measure' / 'cold', 'cold outside owned comparison cold root')
    config = read_pinned(request['config'])
    native = checked_path(config['output_directory'])
    require(native == comparison / 'measure' / 'hot' / 'matched-native', 'native outside exact owned matched path')
    require(len(config['iterations']) == 162 and all(len(i['episodes']) == 10 for i in config['iterations']),
            'full unchanged 162 x 10 comparison required')
    report_pin = pin(dispatch_root / 'report.json')
    report = read_pinned(report_pin)
    require(report['complete'] and not report['qualification'] and report['completed_updates'] == 162
            and report['completed_games'] == 1620 and report['request'] == request_pin,
            'full report/request coverage differs')
    require(len(report['fingerprint']['iterations']) == 162, 'missing full optimizer fingerprint')
    execution = read_pinned(report['execution'])
    require(execution['exit_code'] == 0 and execution['error'] is None, 'native execution failed')
    archive = read_pinned(report['archive'])
    require(archive['scheme'] == 'two-deflate1-shards-full-readback/v1' and archive['mismatches'] == 0
            and len(archive['shards']) == 2 and Path(archive['native_root']).resolve() == native,
            'recovery archive does not bind this native case')
    for shard in archive['shards']:
        require(pin(shard['archive']['path']) == shard['archive'], 'recovery archive changed')
    out = checked_path(args.out)
    require(out.is_relative_to(comparison / 'measure') and not out.is_relative_to(native)
            and not native.is_relative_to(out), 'maintenance output must be a separate owned measure subtree')
    return comparison, request_pin, request, dispatch_root, cold, native, report_pin, report, out


def inspect(args):
    began = time.monotonic()
    _, request_pin, request, dispatch_root, cold, native, report_pin, report, out = inputs(args)
    require(not out.exists(), 'preserve existing inspection; choose a new output root')
    out.mkdir(parents=True)
    decks = checked_path(args.decks)
    t1 = checked_path(args.t1)
    require(pin(t1)['sha256'] == read_pinned(request['config'])['initial_source']['checkpoint']['sha256'],
            'T1 does not match frozen initial checkpoint')
    tools = checked_path(args.launcher_root) / 'python' / 'tools'
    sys.path.insert(0, str(tools))
    collector = importlib.import_module('nine_deck_exposure_collector_v1')
    campaign = importlib.import_module('nine_deck_campaign_v1')
    timers = {}
    phase = time.monotonic()
    exposure = out / 'exposure'
    exposure.mkdir()
    summary = collector.collect('r5', 1, native, decks, exposure)
    require(summary['verdict'] == 'pass' and all(row['pass'] for row in summary['checks'].values()),
            'exposure checks failed')
    timers['exposure_seconds'] = time.monotonic() - phase
    summary_pin = pin(exposure / 'r5-b01-exposure.json')
    phase = time.monotonic()
    completion = json.loads((native / 'completion.json').read_bytes())
    require(completion['complete'] and completion['completed_iterations'] == 162, 'native completion differs')
    last = read_pinned(completion['iterations'][-1])
    update = read_pinned(last['update'])
    source_pin = update['checkpoint']
    require(source_pin['sha256'] == summary['final_checkpoint_sha256'] and pin(source_pin['path']) == source_pin,
            'final checkpoint differs from exposure')
    checkpoint = out / 'final-checkpoint.json'
    shutil.copyfile(source_pin['path'], checkpoint)
    checkpoint_pin = pin(checkpoint)
    require(checkpoint_pin['sha256'] == source_pin['sha256'], 'copied final checkpoint differs')
    timers['final_checkpoint_copy_verify_seconds'] = time.monotonic() - phase
    phase = time.monotonic()
    gate = collector.embedding_gate(json.loads(t1.read_bytes()), json.loads(checkpoint.read_bytes()),
                                    collector.ndb.load_decks(decks))
    require(gate['pass'], 'embedding gate failed')
    gate_pin = save(out / 'embedding-gate.json', gate)
    timers['embedding_gate_seconds'] = time.monotonic() - phase
    phase = time.monotonic()
    physical = {label: physical_stats(path) for label, path in
                [('native', native), ('cold', cold), ('dispatch', dispatch_root), ('maintenance', out)]}
    timers['physical_accounting_seconds'] = time.monotonic() - phase
    phase = time.monotonic()
    entries = []
    for label, folder in [('cold', cold), ('dispatch', dispatch_root), ('maintenance', out)]:
        for source in files_in_tree(folder):
            entries.append({'source': pin(source), 'bytes': source.stat().st_size,
                            'relative_destination': str(Path(label) / source.relative_to(folder))})
    plan = {'schema': 'training-speedup-independent-cold-copy-plan/v1', 'source_host': platform.node(),
            'destination_host': 'DESKTOP-DJ1C40R', 'destination_drive': 'E:', 'request': request_pin,
            'report': report_pin, 'native_root': str(native), 'files': entries,
            'requires_full_destination_sha256_verification': True}
    plan_pin = save(out / 'recovery-copy-plan.json', plan)
    timers['recovery_copy_plan_hash_seconds'] = time.monotonic() - phase
    result = {'schema': 'training-speedup-maintenance-inspect/v1', 'complete': True,
              'request': request_pin, 'report': report_pin, 'decks': pin(decks), 't1': pin(t1),
              'exposure': summary_pin, 'final_checkpoint': checkpoint_pin, 'embedding_gate': gate_pin,
              'retained_updates': summary['retained_updates'], 'native_root': str(native),
              'recovery_copy_plan': plan_pin, 'physical': physical, 'timing': timers,
              'seconds': time.monotonic() - began,
              'retention_preserves': 'root files, small receipts, seeded 2% trajectories and separate final checkpoint; all full intermediate checkpoints in independent recovery ZIPs',
              'maintenance_sources': {'collector': pin(tools / 'nine_deck_exposure_collector_v1.py'),
                                      'retainer': pin(tools / 'nine_deck_campaign_v1.py')}}
    saved = save(out / 'inspect.json', result)
    print(json.dumps({'complete': True, 'inspection': saved, 'recovery_copy_plan': plan_pin}))


def retain(args):
    began = time.monotonic()
    _, request_pin, _, _, _, native, report_pin, _, out = inputs(args)
    require(args.cold_copy_receipt and args.cold_copy_sha256, 'explicit pinned independent copy receipt required')
    require(not (out / 'retain.json').exists(), 'retention already recorded')
    inspection = read_pinned(pin(out / 'inspect.json'))
    require(inspection['complete'] and inspection['request'] == request_pin and inspection['report'] == report_pin
            and inspection['native_root'] == str(native), 'inspection differs from case')
    for key in ('decks', 't1', 'exposure', 'final_checkpoint', 'embedding_gate'):
        require(pin(inspection[key]['path']) == inspection[key], 'inspection artifact changed: ' + key)
    plan_pin = inspection['recovery_copy_plan']
    plan = read_pinned(plan_pin)
    receipt_pin = {'path': str(checked_path(args.cold_copy_receipt)), 'sha256': args.cold_copy_sha256}
    receipt = read_pinned(receipt_pin)
    require(receipt['schema'] == 'training-speedup-independent-cold-copy/v1' and receipt['complete'] is True
            and receipt['plan'] == plan_pin and receipt['source_host'] == plan['source_host'] == platform.node()
            and receipt['destination_host'].upper() == plan['destination_host']
            and receipt['destination_host'].upper() != platform.node().upper(), 'independent cold copy identity differs')
    require(len(receipt['files']) == len(plan['files']), 'independent recovery coverage differs')
    destinations = set()
    for planned, copied in zip(plan['files'], receipt['files']):
        require(copied['source'] == planned['source'] and copied['bytes'] == planned['bytes']
                and copied['verified'] is True and copied['destination']['sha256'] == planned['source']['sha256'],
                'independent recovery checksum/coverage differs')
        destination = PureWindowsPath(copied['destination']['path'])
        require(destination.is_absolute() and destination.drive.upper() == 'E:' and '..' not in destination.parts,
                'independent recovery must be on desktop E:')
        require(str(destination).casefold() not in destinations, 'duplicate recovery destination')
        destinations.add(str(destination).casefold())
        require(pin(planned['source']['path']) == planned['source'], 'remote recovery source changed')
    verification_seconds = time.monotonic() - began
    files_in_tree(native)  # Refuse every nested reparse/link before recursive pruning.
    tools = checked_path(args.launcher_root) / 'python' / 'tools'
    sys.path.insert(0, str(tools))
    campaign = importlib.import_module('nine_deck_campaign_v1')
    require(pin(tools / 'nine_deck_campaign_v1.py') == inspection['maintenance_sources']['retainer'],
            'maintenance implementation changed')
    retained = out / 'retained'
    require(not retained.exists(), 'preserve partial retention; manual investigation required')
    phase = time.monotonic()
    manifest = campaign.retain_block(SimpleNamespace(retained=retained, prune_log=out / 'PRUNE.jsonl'),
                                     'r5', 1, native, read_pinned(inspection['exposure']))
    retention_seconds = time.monotonic() - phase
    result = {'schema': 'training-speedup-maintenance-retain/v1', 'complete': True,
              'request': request_pin, 'inspection': pin(out / 'inspect.json'),
              'independent_cold_copy': receipt_pin, 'retained_manifest': manifest,
              'prune_log': pin(out / 'PRUNE.jsonl'), 'retained_physical': physical_stats(retained),
              'timing': {'independent_copy_receipt_and_source_verify_seconds': verification_seconds,
                         'retention_verify_prune_seconds': retention_seconds},
              'seconds': time.monotonic() - began, 'finished_utc': datetime.now(timezone.utc).isoformat()}
    saved = save(out / 'retain.json', result)
    print(json.dumps({'complete': True, 'retention': saved}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=('inspect', 'retain'))
    parser.add_argument('--launcher-root', type=Path, required=True)
    parser.add_argument('--comparison-root', type=Path, default=Path('C:/mtg-node/training-speedups-20261009'))
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--decks', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--t1', type=Path, required=True)
    parser.add_argument('--cold-copy-receipt', type=Path)
    parser.add_argument('--cold-copy-sha256')
    args = parser.parse_args()
    (inspect if args.stage == 'inspect' else retain)(args)


if __name__ == '__main__':
    main()
