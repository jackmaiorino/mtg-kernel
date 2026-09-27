"""Write the per-member admission receipt the line (a) collector consumes (strict or R14 route).

The receipt binds exactly the bytes the shared loader reads (the ExpandedModelSourceV1 file and the
import descriptor it pins), the expected effective identity, the Legacy adapter flags, the registry
pins and the member's acceptance evidence (round-trip and smoke receipts). The collector verifies
these bytes, loads the model source through load_expanded_inference_v1, and compares the loaded
identity with expected_identity before collection. A receipt is written only for a member whose
round trip passed and whose smoke completed; it never admits an unaccepted route by itself.

Usage: python python/tools/line_a_panel_admission_v1.py LABEL
"""
import hashlib, json, sys
from pathlib import Path

SEALED = Path('E:/mtg-line-a-panel-imports-20260927')
V3 = {'feature_contract_digest': '9319fbd41e6b42ec90d565c13f3b4f75a3898dafe3816387453c08419ba9cc68',
      'feature_encoding_digest': 'c4662291ca9a75525b51b51f3b5d512671340c05b69fd33fb0827c0c8af70a2b'}
R14_IMPORT_SCHEMA = 'mtg-kernel-frozen-play-registry-evolution-import/v1'
IDENTITY_SCHEMAS = {'strict': 'mtg-kernel-frozen-sideboard-play-transfer/v1',
                    'r14': 'mtg-kernel-frozen-sideboard-play-registry-evolution-transfer/v1'}


def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main(label):
    name = label.replace('/', '__')
    receipts = SEALED / 'receipts' / name
    roundtrip, smoke = json.loads((receipts / 'roundtrip.json').read_bytes()), json.loads((receipts / 'smoke.json').read_bytes())
    if not (roundtrip.get('passed') is True and smoke.get('completed') is True):
        raise SystemExit('member has no passing round trip and completed smoke')
    model_source_path = SEALED / 'descriptors' / name / 'model-source.json'
    model_source = json.loads(model_source_path.read_bytes())
    if model_source.get('checkpoint') is not None or model_source['feature_transfer'] != {
            'expected_feature_contract_digest': V3['feature_contract_digest'], 'expected_feature_encoding_digest': V3['feature_encoding_digest']}:
        raise SystemExit('model source is not an inference-only explicit V3 transfer')
    import_path = Path(model_source['play_import']['path'])
    descriptor = json.loads(import_path.read_bytes())
    if sha(import_path) != model_source['play_import']['sha256']:
        raise SystemExit('import descriptor pin differs')
    route = 'r14' if descriptor.get('schema') == R14_IMPORT_SCHEMA else 'strict'
    identity = roundtrip['inference_identity']
    source_import = identity['source_import']
    source_import = source_import.get('Imported', source_import)
    if source_import.get('schema') != IDENTITY_SCHEMAS[route]:
        raise SystemExit('loaded identity schema does not match the descriptor route')
    weights = [m['weights_sha256'] for m in smoke['models']]
    if len(set(weights)) != 1 or weights[0] != identity['model']['weights_sha256']:
        raise SystemExit('smoke seats and round-trip identity disagree on weights')
    receipt = {
        'schema': 'mtg-kernel-line-a-panel-admission/v1',
        'member': label,
        'route': route,
        'model_source': {'path': model_source_path.as_posix(), 'sha256': sha(model_source_path)},
        'import_descriptor': {'path': import_path.as_posix(), 'sha256': sha(import_path), 'schema': descriptor.get('schema')},
        'expected_identity': {'identity_schema': IDENTITY_SCHEMAS[route],
                              'model_parameter_sha256': identity['model']['model_parameter_sha256'],
                              'weights_sha256': identity['model']['weights_sha256'],
                              'feature_generation': 'V3', 'observation_successor': True, **V3},
        'adapter_flags': {'v3_forced_actions': True, 'v3_spell_target_reference_adapter': True},
        'registry_pins': {'source_registry_sha256': descriptor['expected_source_registry_sha256'],
                          'destination_registry_sha256': source_import['destination_registry_sha256'],
                          'destination_card_db_hash': descriptor['expected_destination_card_db_hash'],
                          'allowlist_sha256': descriptor.get('expected_allowlist_sha256')},
        'evidence': {'roundtrip_receipt': {'path': (receipts / 'roundtrip.json').as_posix(), 'sha256': sha(receipts / 'roundtrip.json')},
                     'smoke_receipt': {'path': (receipts / 'smoke.json').as_posix(), 'sha256': sha(receipts / 'smoke.json')},
                     'exporter_build_commit': descriptor['source_registry_git_commit'],
                     'evaluator_sha256': smoke['evaluator_sha256'], 'evaluator_git_head': smoke['evaluator_git_head']},
        'nonclaims': ['engineering admission of a route-accepted import; not a strength claim and not roster authority',
                      'no outcome, winner or score was read'],
    }
    data = (json.dumps(receipt, indent=2) + '\n').encode('utf-8')
    path = SEALED / 'admission' / (name + '.json')
    path.parent.mkdir(exist_ok=True)
    if path.exists():
        raise SystemExit('refusing to overwrite ' + path.as_posix())
    path.write_bytes(data)
    print(path.as_posix(), hashlib.sha256(data).hexdigest(), route)


if __name__ == '__main__':
    main(sys.argv[1])
