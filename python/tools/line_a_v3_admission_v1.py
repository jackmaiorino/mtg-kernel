"""Admission receipt for the frozen V3 (the D3 envelope's Legacy opponent) at one build.

The V3 transfer envelope records the destination build commit, and the loader
reproduces the envelope, so the V3 loads only in a binary of that commit.
Rebinding (g115_d3_payload_v1.prepare) changes only
receipt.destination_build_git_head. This tool checks a rebound descriptor
against the D3 declaration's pinned inputs (every other byte and pin equal)
and writes the receipt the public collector's Legacy kind requires: schema
mtg-kernel-line-a-panel-admission/v1, route v3-frozen, adapter flags true/true.
The expected identity comes from pinned D3 records, not literals: model
parameters and weights from the D3 formal collection's actual-play model
record, feature digests from the D3 declaration, both cross-checked against
the envelope's transfer receipt. The receipt binds that identity, the rebind
provenance and the rebound model-source bytes; the collector compares the
loaded identity with it before collection. Metadata only: no engine, model
load or outcome read.

Usage: line_a_v3_admission_v1.py --model-source FILE --commit SHA --out FILE
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path

PAYLOAD = Path('E:/mtg-g115-lineage-20260923/d3-attempt004-preparation-001/haley-binding/payload')
# The two anchors: the D3 declaration and the D3 formal collection record.
D3_DECLARATION = (PAYLOAD / 'sources.json', 'b1ad4fa11eb300cf0d3a729ce19f26e8fcfee0e95b23204ee88b82f17b26155f')
D3_EXECUTION = (Path('E:/mtg-g115-lineage-20260923/d3-attempt004-collection-001/execution.json'),
                '00d43b3202380f7dc8b928ac1ac159081ec1a937089853b07aa18cef55c040af')
DESCRIPTOR_SCHEMA = 'mtg-kernel-expanded-registry-transfer-source/v1'
# Loaded identity schema and observation successor of that descriptor schema
# (the collector's v3-frozen route implies the same pair).
IDENTITY_SCHEMA = 'mtg-kernel-registry-transferred-play/v1'
PIN_FIELDS = ('source_checkpoint', 'source_registry', 'continuation_schedule')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned_json(path, digest):
    path = Path(path)
    assert sha(path) == digest, f'pinned file differs: {path}'
    return json.loads(path.read_bytes())


def ref(path):
    return {'path': str(Path(path).resolve()).replace('\\', '/'), 'sha256': sha(path)}


def d3_expectations():
    """The declaration's opponent pins and the loaded V3 identity of the D3 run."""
    declaration = pinned_json(*D3_DECLARATION)
    opponent = declaration['opponent']
    assert opponent['kind'] == 'legacy' and opponent['v3_forced_actions'] and opponent['v3_spell_target_reference_adapter']
    features = opponent['source']['feature_transfer']
    original_descriptor = pinned_json(PAYLOAD / 'inputs/v3-play-import-source.json', opponent['source']['play_import']['sha256'])
    assert original_descriptor['schema'] == DESCRIPTOR_SCHEMA
    change = declaration['v3_provenance_change']
    assert change['only_changed_field'] == 'receipt.destination_build_git_head'
    assert original_descriptor['transfer_envelope']['sha256'] == change['new']['sha256']
    envelope_path = PAYLOAD / 'inputs/v3-transfer-envelope.json'
    envelope = pinned_json(envelope_path, change['new']['sha256'])
    receipt = envelope['receipt']
    assert receipt['destination_build_git_head'] == change['to_build']
    assert (receipt['features']['contract_digest'], receipt['features']['encoding_digest']) == (
        features['expected_feature_contract_digest'], features['expected_feature_encoding_digest'])
    execution = pinned_json(*D3_EXECUTION)
    loaded = [(i, m) for i, m in enumerate(execution['models'])
              if (m.get('feature_contract_digest'), m.get('feature_encoding_digest')) == (
                  features['expected_feature_contract_digest'], features['expected_feature_encoding_digest'])
              and m.get('model_parameter_sha256') == receipt['destination_model_parameter_sha256']]
    assert len(loaded) == 1, 'the D3 record must hold exactly one model with the declared V3 identity'
    index, model = loaded[0]
    identity = {'identity_schema': IDENTITY_SCHEMA,
                'model_parameter_sha256': model['model_parameter_sha256'],
                'weights_sha256': model['weights_sha256'],
                'feature_generation': 'V3', 'observation_successor': True,
                'feature_contract_digest': features['expected_feature_contract_digest'],
                'feature_encoding_digest': features['expected_feature_encoding_digest']}
    provenance = {'d3_declaration': {'path': D3_DECLARATION[0].as_posix(), 'sha256': D3_DECLARATION[1]},
                  'd3_execution': {'path': D3_EXECUTION[0].as_posix(), 'sha256': D3_EXECUTION[1], 'models_index': index},
                  'd3_envelope': {'path': envelope_path.as_posix(), 'sha256': change['new']['sha256'], 'build': change['to_build']}}
    return original_descriptor, envelope, features, identity, provenance


def admission(model_source_path, commit):
    assert len(commit) == 40 and all(c in '0123456789abcdef' for c in commit), 'full lowercase commit SHA'
    original_descriptor, original_envelope, features, identity, provenance = d3_expectations()
    source = json.loads(Path(model_source_path).read_bytes())
    assert set(source) == {'play_import', 'feature_transfer', 'checkpoint'} and source['checkpoint'] is None
    assert source['feature_transfer'] == features
    descriptor = pinned_json(source['play_import']['path'], source['play_import']['sha256'])
    assert set(descriptor) == set(original_descriptor) and descriptor['schema'] == DESCRIPTOR_SCHEMA
    for key in PIN_FIELDS:
        digest = original_descriptor[key]['sha256']
        assert descriptor[key]['sha256'] == digest and sha(descriptor[key]['path']) == digest, key
    envelope = pinned_json(descriptor['transfer_envelope']['path'], descriptor['transfer_envelope']['sha256'])
    expected = copy.deepcopy(original_envelope)
    expected['receipt']['destination_build_git_head'] = commit
    assert envelope == expected, 'rebound envelope differs from the D3 original beyond the build commit'
    return {
        'schema': 'mtg-kernel-line-a-panel-admission/v1',
        'member': 'v3',
        'route': 'v3-frozen',
        'model_source': ref(model_source_path),
        'import_descriptor': {**source['play_import'], 'schema': DESCRIPTOR_SCHEMA},
        'expected_identity': identity,
        'adapter_flags': {'v3_forced_actions': True, 'v3_spell_target_reference_adapter': True},
        'registry_pins': {**{k: descriptor[k] for k in PIN_FIELDS}, 'transfer_envelope': descriptor['transfer_envelope']},
        'evidence': {**provenance,
                     'rebind': {'from_build': original_envelope['receipt']['destination_build_git_head'], 'to_build': commit,
                                'only_changed_field': 'receipt.destination_build_git_head',
                                'procedure': 'g115_d3_payload_v1.prepare'},
                     'tool': ref(__file__)},
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
