"""Admission receipt for the frozen V3 (the D3 envelope's Legacy opponent) at one build.

The V3 transfer envelope records the destination build commit, and the loader
reproduces the envelope, so the V3 loads only in a binary of that commit.
Rebinding (g115_d3_payload_v1.prepare) changes only
receipt.destination_build_git_head. This tool checks a rebound descriptor
against the D3 original (every other byte and pin equal) and writes the
receipt the public collector's Legacy kind requires: schema
mtg-kernel-line-a-panel-admission/v1, route v3-frozen, adapter flags true/true.
The expected identity is build-independent (legacy_admission_identity_probe at
13759e4d); the collector compares it with the loaded identity before
collection. Metadata only: no engine, model load or outcome read.

Usage: line_a_v3_admission_v1.py --model-source FILE --commit SHA --out FILE
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path

D3_ENVELOPE = Path('E:/mtg-g115-lineage-20260923/d3-attempt004-preparation-001/haley-binding/payload/inputs/v3-transfer-envelope.json')
D3_ENVELOPE_SHA256 = '5747b4e861de213f01ddfb2525af6ef23e3196d1bf88bb03f67b1dd5aa4f9276'
D3_BUILD = 'cd41885e0ac05586d89bd4b2b7fb1284248689ef'
DESCRIPTOR_SCHEMA = 'mtg-kernel-expanded-registry-transfer-source/v1'
PINS = {'source_checkpoint': '51ce5ddfea11836364e6068440135e89d94c49782ba514e8442ad5d4cbaa690d',
        'source_registry': '561aa07cbe789d53ff330ba0b53589338eaca54caba27139d2980be517d9db5a',
        'continuation_schedule': '36c0e8afa996b78ca21eca9c33dc2b353101cfb484820da8f50d4e3637898e13'}
FEATURES = {'expected_feature_contract_digest': '9319fbd41e6b42ec90d565c13f3b4f75a3898dafe3816387453c08419ba9cc68',
            'expected_feature_encoding_digest': 'c4662291ca9a75525b51b51f3b5d512671340c05b69fd33fb0827c0c8af70a2b'}
EXPECTED_IDENTITY = {'identity_schema': 'mtg-kernel-registry-transferred-play/v1',
                     'model_parameter_sha256': '8474051a1055125c9d3d45fad77c626112868a83b9b4594c1a900b64075a3486',
                     'weights_sha256': '2fa88edf3c8f6170f721f6462b1297a70e71daaa94d7a36e43117f83d43453eb',
                     'feature_generation': 'V3', 'observation_successor': True,
                     'feature_contract_digest': FEATURES['expected_feature_contract_digest'],
                     'feature_encoding_digest': FEATURES['expected_feature_encoding_digest']}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned(ref):
    path = Path(ref['path'])
    assert path.is_absolute() and sha(path) == ref['sha256'], f'pin differs: {path}'
    return json.loads(path.read_bytes())


def admission(model_source_path, commit):
    assert len(commit) == 40 and all(c in '0123456789abcdef' for c in commit), 'full lowercase commit SHA'
    source = json.loads(Path(model_source_path).read_bytes())
    assert set(source) == {'play_import', 'feature_transfer', 'checkpoint'} and source['checkpoint'] is None
    assert source['feature_transfer'] == FEATURES
    descriptor = pinned(source['play_import'])
    assert set(descriptor) == {'schema', *PINS, 'transfer_envelope'} and descriptor['schema'] == DESCRIPTOR_SCHEMA
    for key, digest in PINS.items():
        assert descriptor[key]['sha256'] == digest and sha(descriptor[key]['path']) == digest, key
    envelope = pinned(descriptor['transfer_envelope'])
    assert sha(D3_ENVELOPE) == D3_ENVELOPE_SHA256
    original = json.loads(D3_ENVELOPE.read_bytes())
    assert original['receipt']['destination_build_git_head'] == D3_BUILD
    expected = copy.deepcopy(original)
    expected['receipt']['destination_build_git_head'] = commit
    assert envelope == expected, 'rebound envelope differs from the D3 original beyond the build commit'
    return {
        'schema': 'mtg-kernel-line-a-panel-admission/v1',
        'member': 'v3',
        'route': 'v3-frozen',
        'model_source': {'path': str(Path(model_source_path).resolve()).replace('\\', '/'), 'sha256': sha(model_source_path)},
        'import_descriptor': {**source['play_import'], 'schema': DESCRIPTOR_SCHEMA},
        'expected_identity': EXPECTED_IDENTITY,
        'adapter_flags': {'v3_forced_actions': True, 'v3_spell_target_reference_adapter': True},
        'registry_pins': {**{k: descriptor[k] for k in PINS}, 'transfer_envelope': descriptor['transfer_envelope']},
        'evidence': {'d3_envelope': {'path': str(D3_ENVELOPE).replace('\\', '/'), 'sha256': D3_ENVELOPE_SHA256, 'build': D3_BUILD},
                     'rebind': {'to_build': commit, 'only_changed_field': 'receipt.destination_build_git_head',
                                'procedure': 'g115_d3_payload_v1.prepare'},
                     'identity_probe': 'legacy_admission_identity_probe at 13759e4d; the collector checks the loaded identity'},
        'nonclaims': ['engineering admission of the frozen V3 at one build commit; not a strength claim and not roster authority',
                      'no outcome, winner or score was read'],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--model-source', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--out', required=True)
    args = parser.parse_args()
    out = Path(args.out)
    assert not out.exists(), f'refusing to overwrite {out}'
    data = json.dumps(admission(args.model_source, args.commit), indent=1).encode('ascii')
    out.write_bytes(data)
    print(json.dumps({'out': str(out), 'sha256': hashlib.sha256(data).hexdigest(), 'commit': args.commit}))


if __name__ == '__main__':
    main()
