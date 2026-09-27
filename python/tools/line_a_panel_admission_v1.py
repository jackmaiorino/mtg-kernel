"""Write the per-member admission receipt the line (a) collector consumes (strict or R14 route).

The receipt binds exactly the bytes the shared loader reads (the ExpandedModelSourceV1 file and the
import descriptor it pins), the expected nested identity (identity schema, namespace rule, source
registry and card-DB hash, model and weights digests), the Legacy adapter flags as the smoke actually
ran them, the registry pins and the acceptance evidence (round-trip and smoke receipts). The collector
verifies these bytes, loads the model source through load_expanded_inference_v1, compares the loaded
identity with expected_identity, and tells routes apart by the nested identity schema plus this
receipt's route, never by the evaluator's outer receipt schema.

An R14 receipt additionally requires an acceptance reference (FABLE-REVIEW-20260927 R14 verdict and
CODEX #566): the Fable record section, Codex's implementation countersign note and the accepted
commit. The smoke evaluator must be built at that commit. A receipt never admits an unaccepted route.

Usage: python python/tools/line_a_panel_admission_v1.py LABEL
           [--fable-record PATH --fable-section HEADING --codex-note 'CODEX #N' --accepted-commit SHA]
"""
import argparse, hashlib, json
from pathlib import Path

SEALED = Path('E:/mtg-line-a-panel-imports-20260927')
V3 = {'feature_contract_digest': '9319fbd41e6b42ec90d565c13f3b4f75a3898dafe3816387453c08419ba9cc68',
      'feature_encoding_digest': 'c4662291ca9a75525b51b51f3b5d512671340c05b69fd33fb0827c0c8af70a2b'}
R14_IMPORT_SCHEMA = 'mtg-kernel-frozen-play-registry-evolution-import/v1'
IDENTITY_SCHEMAS = {'strict': 'mtg-kernel-frozen-sideboard-play-transfer/v1',
                    'r14': 'mtg-kernel-frozen-sideboard-play-registry-evolution-transfer/v1'}
R14_SOURCE_CARD_DB_HASH = 'a06fa9566106f0ea'


def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fail(message): raise SystemExit('admission refused: ' + message)


def review_section(record, heading):
    text = Path(record).read_text(encoding='utf-8')
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines) if line.startswith('## ') and line.startswith(heading)), None)
    if start is None:
        fail('Fable record has no section starting with the given heading')
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith('## ') or lines[i].startswith('# ')), len(lines))
    body = '\n'.join(lines[start:end]).rstrip() + '\n'
    return {'record': Path(record).as_posix(), 'heading': lines[start][:200],
            'section_sha256': hashlib.sha256(body.encode('utf-8')).hexdigest()}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('label')
    parser.add_argument('--fable-record')
    parser.add_argument('--fable-section')
    parser.add_argument('--codex-note')
    parser.add_argument('--accepted-commit')
    args = parser.parse_args()
    name = args.label.replace('/', '__')
    receipts = SEALED / 'receipts' / name
    roundtrip, smoke = json.loads((receipts / 'roundtrip.json').read_bytes()), json.loads((receipts / 'smoke.json').read_bytes())
    if not (roundtrip.get('passed') is True and smoke.get('completed') is True):
        fail('member has no passing round trip and completed smoke')
    model_source_path = SEALED / 'descriptors' / name / 'model-source.json'
    model_source = json.loads(model_source_path.read_bytes())
    if model_source.get('checkpoint') is not None or model_source['feature_transfer'] != {
            'expected_feature_contract_digest': V3['feature_contract_digest'], 'expected_feature_encoding_digest': V3['feature_encoding_digest']}:
        fail('model source is not an inference-only explicit V3 transfer')
    import_path = Path(model_source['play_import']['path'])
    if sha(import_path) != model_source['play_import']['sha256']:
        fail('import descriptor pin differs')
    descriptor = json.loads(import_path.read_bytes())
    schema = descriptor.get('schema')
    if schema is None:
        route = 'strict'
    elif schema == R14_IMPORT_SCHEMA:
        route = 'r14'
    else:
        fail('unknown import descriptor schema %r' % schema)
    identity = roundtrip['inference_identity']
    nested = identity['source_import']
    nested = nested.get('Imported', nested)
    if nested.get('schema') != IDENTITY_SCHEMAS[route]:
        fail('loaded identity schema does not match the descriptor route')
    seats = smoke.get('models') or []
    if len(seats) != 2:
        fail('smoke receipt lacks both seat models')
    weights = {seat['weights_sha256'] for seat in seats}
    if weights != {identity['model']['weights_sha256']}:
        fail('smoke seats and round-trip identity disagree on weights')
    flags = [(seat.get('v3_forced_actions'), seat.get('v3_spell_target_reference_adapter')) for seat in seats]
    if len(set(flags)) != 1 or flags[0] != (True, True):
        fail('smoke did not run both seats with the Legacy V3 adapter flags true')
    seat_schemas = {(seat.get('identity') or {}).get('source_import', {}).get('schema') for seat in seats}
    if seat_schemas != {IDENTITY_SCHEMAS[route]}:
        fail('smoke seat identities do not carry the route identity schema')
    acceptance = None
    if route == 'r14':
        if not (args.fable_record and args.fable_section and args.codex_note and args.accepted_commit):
            fail('an R14 receipt requires the Fable section, the Codex implementation note and the accepted commit')
        if smoke.get('evaluator_git_head') != args.accepted_commit:
            fail('the smoke evaluator was not built at the accepted commit')
        if nested.get('source_card_db_hash') != R14_SOURCE_CARD_DB_HASH:
            fail('source card-DB hash is not the pinned R14 source')
        if descriptor['expected_allowlist_sha256'] not in nested.get('namespace_rule', ''):
            fail('namespace rule does not name the pinned allowlist')
        acceptance = {'fable_verdict': review_section(args.fable_record, args.fable_section),
                      'codex_implementation_note': args.codex_note, 'accepted_commit': args.accepted_commit}
    receipt = {
        'schema': 'mtg-kernel-line-a-panel-admission/v2',
        'member': args.label,
        'route': route,
        'acceptance': acceptance,
        'model_source': {'path': model_source_path.as_posix(), 'sha256': sha(model_source_path)},
        'import_descriptor': {'path': import_path.as_posix(), 'sha256': sha(import_path), 'schema': schema},
        'expected_identity': {'identity_schema': nested['schema'], 'namespace_rule': nested['namespace_rule'],
                              'source_registry_sha256': nested['source_registry_sha256'],
                              'source_card_db_hash': nested['source_card_db_hash'],
                              'destination_registry_sha256': nested['destination_registry_sha256'],
                              'destination_card_db_hash': nested['destination_card_db_hash'],
                              'model_parameter_sha256': identity['model']['model_parameter_sha256'],
                              'weights_sha256': identity['model']['weights_sha256'],
                              'feature_generation': 'V3', 'observation_successor': True, **V3},
        'adapter_flags': {'v3_forced_actions': flags[0][0], 'v3_spell_target_reference_adapter': flags[0][1],
                          'source': 'smoke receipt seat models'},
        'registry_pins': {'source_registry_sha256': descriptor['expected_source_registry_sha256'],
                          'destination_card_db_hash': descriptor['expected_destination_card_db_hash'],
                          'allowlist_sha256': descriptor.get('expected_allowlist_sha256')},
        'evidence': {'roundtrip_receipt': {'path': (receipts / 'roundtrip.json').as_posix(), 'sha256': sha(receipts / 'roundtrip.json')},
                     'smoke_receipt': {'path': (receipts / 'smoke.json').as_posix(), 'sha256': sha(receipts / 'smoke.json')},
                     'exporter_build_commit': descriptor['source_registry_git_commit'],
                     'evaluator_sha256': smoke['evaluator_sha256'], 'evaluator_git_head': smoke['evaluator_git_head']},
        'reader_rule': 'distinguish routes by expected_identity.identity_schema plus route, never by the evaluator outer receipt schema',
        'nonclaims': ['engineering admission of a route-accepted import; not a strength claim and not roster authority',
                      'no outcome, winner or score was read'],
    }
    data = (json.dumps(receipt, indent=2) + '\n').encode('utf-8')
    path = SEALED / 'admission' / 'v2' / (name + '.json')
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        fail('refusing to overwrite ' + path.as_posix())
    path.write_bytes(data)
    print(path.as_posix(), hashlib.sha256(data).hexdigest(), route)


if __name__ == '__main__':
    main()
