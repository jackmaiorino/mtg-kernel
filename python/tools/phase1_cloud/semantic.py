"""Exact trajectory comparison with only validated model-source relocation.

No recursive path/hash stripping. Only the three ExpandedModelSourceV1 slots in
the v2/v3 trajectory are rewritten, and only for exact sources in the pinned map.
All identities, tensors, actions, observations, probabilities and terminal data
remain in the canonical digest. This is engineering equivalence, not strength.
"""
from __future__ import annotations
import copy
import hashlib
from pathlib import Path, PurePosixPath
from common import encoded, pin, read, relative, require
from fresh_source import (SOURCE_SCHEMA, inspect_source, is_fresh_origin, origin_key, validate_checkpoint_origin,
                          validate_inference_identity, validate_trajectory)

ALGORITHM = 'expanded-trajectory-v2-v3-exact-model-source-relocation/v2'
IMPORT_PATHS = ('export_directory', 'source_run_path', 'source_registry_path')


def sources(config):
    return [config['initial_source'], *[item['source'] for item in config['opponents']]]


def payload_file(payload_pin, manifest, reference):
    """Read the staged or recovered payload without following arbitrary paths."""
    remote = PurePosixPath(manifest['remote_root'])
    try:
        name = str(relative(str(PurePosixPath(reference['path']).relative_to(remote))))
    except ValueError as error:
        raise ValueError('relocation evidence outside payload') from error
    require(name in manifest['files'] and
            manifest['files'][name]['sha256'] == reference['sha256'], 'unlisted relocation evidence')
    # The manifest may have been downloaded beside a complete recovered payload.
    local = Path(payload_pin['path']).parent / name
    pin(local, reference['sha256'])
    return local


def validate_model_source_relocations(payload_pin, manifest):
    pin(payload_pin['path'], payload_pin['sha256'])
    require(read(payload_pin['path']) == manifest and manifest['schema'] in
            ('phase1-cloud-payload/v1','phase1-bo3-cloud-payload/v1'), 'wrong pinned relocation payload')
    rows = manifest['model_source_relocations']
    require(rows and isinstance(rows, list), 'exact model-source relocation map missing')
    for row in rows:
        original, relocated = row['original'], row['relocated']
        require(set(original) == set(relocated), 'model-source fields changed during relocation')
        original_import = row['original_import_file']
        require(original_import['sha256'] == original['play_import']['sha256'],
                'original import copy does not match source')
        a = read(payload_file(payload_pin, manifest, original_import))
        b = read(payload_file(payload_pin, manifest, relocated['play_import']))
        def remote_pin(path, digest):
            return payload_file(payload_pin, manifest, {'path': path, 'sha256': digest})
        expected = copy.deepcopy(a)
        fresh = a.get('schema') == SOURCE_SCHEMA
        verified = None
        if fresh:
            require(set(a) == set(b) == {'schema','initialization','parameters'} and b['schema'] == SOURCE_SCHEMA,
                    'fresh descriptor schema/fields changed')
            for field in ('initialization','parameters'):
                require(set(a[field]) == set(b[field]) == {'path','sha256'} and a[field]['sha256'] == b[field]['sha256'],
                        'fresh immutable manifest/payload pin changed')
                expected[field]['path'] = b[field]['path']
            verified = inspect_source(relocated['play_import'], relocated['feature_transfer'],
                resolve_pin=lambda ref: payload_file(payload_pin,manifest,ref))
        else:
            for field in IMPORT_PATHS:
                require(field in a and field in b, 'known import path field missing')
                expected[field] = b[field]
            metadata_path = str(PurePosixPath(b['export_directory']) / 'metadata.json')
            metadata = read(remote_pin(metadata_path, a['expected_metadata_sha256']))
            remote_pin(str(PurePosixPath(b['export_directory']) / 'parameters.f32le'), metadata['parameter_section_sha256'])
            remote_pin(b['source_run_path'], metadata['identity']['loaded_run_sha256'])
            remote_pin(b['source_registry_path'], a['expected_source_registry_sha256'])
        require(expected == b, 'play-import changed beyond known path relocation')
        expected_source = copy.deepcopy(original)
        expected_source['play_import'] = relocated['play_import']
        if original.get('checkpoint') is not None:
            require(relocated['checkpoint']['sha256'] == original['checkpoint']['sha256'],
                    'checkpoint changed during relocation')
            checkpoint = payload_file(payload_pin, manifest, relocated['checkpoint'])
            if fresh: validate_checkpoint_origin(read(checkpoint),verified['origin'])
            expected_source['checkpoint'] = relocated['checkpoint']
        require(expected_source == relocated, 'model source changed beyond exact file references')
    return rows


def validated_map(payload_pin, config):
    pin(payload_pin['path'], payload_pin['sha256'])
    manifest = read(payload_pin['path'])
    require(manifest['schema'] == 'phase1-cloud-payload/v1', 'wrong relocation payload schema')
    rows = validate_model_source_relocations(payload_pin,manifest)
    for source in sources(config):
        matches = [row for row in rows if row['relocated'] == source]
        require(matches and all(row['original'] == matches[0]['original'] for row in matches),
                'configuration has missing or ambiguous relocation')
    return rows


def trajectory_digest(document, config, relocations=None, origins=None):
    """Digest one trajectory; `origins` (from fresh_source.fresh_origins) binds every
    fresh behavior's recorded ancestry to the origin derived from its own pinned files."""
    validate_trajectory(document)
    value = copy.deepcopy(document)
    require(len(value['seat_behaviors']) == 2, 'trajectory requires both seat behaviors')
    allowed = sources(config)
    if origins is not None:
        for seat in value['seat_behaviors']:
            identity = seat.get('identity', {})
            if is_fresh_origin(identity.get('source_import')):
                expected = origins.get(origin_key(seat['source']))
                require(expected is not None, 'unproven fresh source origin')
                validate_inference_identity(identity, seat['source'], expected)

    def replace(source):
        require(source in allowed, 'trajectory model source is outside pinned configuration')
        if relocations is None:
            return source
        matches = [row for row in relocations if row['relocated'] == source]
        require(matches and all(row['original'] == matches[0]['original'] for row in matches),
                'trajectory has missing or ambiguous exact source relocation')
        return copy.deepcopy(matches[0]['original'])

    for seat in value['seat_behaviors']:
        seat['source'] = replace(seat['source'])
    value['episode']['opponent'] = replace(value['episode']['opponent'])
    return hashlib.sha256(encoded(value)).hexdigest()
