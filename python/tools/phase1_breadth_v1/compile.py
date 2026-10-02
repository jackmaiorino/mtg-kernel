"""Prepare one explicit breadth block; this command never starts native work."""
import argparse
import hashlib
import json
import os
from pathlib import Path

from catalog_v1 import load_catalog_v1, read_pin, strict_json
from schedule_v1 import compile_block_v1


def write_new(path, value):
    raw = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True, allow_nan=False).encode()
    if len(raw) > 16*1024*1024:
        raise ValueError('Prepared artifact exceeds 16 MiB')
    with path.open('xb') as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    return {'path':path.resolve().as_posix(),'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    with args.spec.open('rb') as stream:
        raw = stream.read(1024*1024+1)
    if len(raw) > 1024*1024:
        raise ValueError('Preparation specification exceeds 1 MiB')
    spec = strict_json(raw)
    if type(spec) is not dict or set(spec) != {'schema','catalog','settings'} or spec['schema'] != 'phase1-breadth-preparation/v1':
        raise ValueError('Invalid breadth preparation specification')
    pins = []
    catalog_spec = read_pin(spec['catalog'], pins, 1024*1024)
    settings = read_pin(spec['settings'], pins, 16*1024*1024)
    catalog = load_catalog_v1(catalog_spec)
    compiled = compile_block_v1(catalog, settings)
    # Compute and validate completely before creating a fresh preparation dir.
    args.output.mkdir(parents=True, exist_ok=False)
    native = write_new(args.output/'native-config.json', compiled['native'])
    report = write_new(args.output/'schedule-report.json', compiled['report'])
    catalog_pin = write_new(args.output/'catalog.json', catalog)
    manifest = {'schema':'phase1-breadth-prepared-block/v1',
        'spec':{'path':args.spec.resolve().as_posix(),'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)},
        'inputs':pins,'native_config':native,'schedule_report':report,'catalog':catalog_pin,
        'native_validation':'required_separately','training_started':False,
        'independent_initializations_created':0,'campaign_launch_authorized':False}
    manifest_pin = write_new(args.output/'manifest.json', manifest)
    print(json.dumps({'manifest':manifest_pin,'training_started':False}))


if __name__ == '__main__':
    main()
