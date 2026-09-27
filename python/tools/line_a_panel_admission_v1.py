"""Write the per-member admission receipt the line (a) collector consumes (strict or R14 route).

The receipt has exactly the shape the collector parses (LegacyAdmissionV1 at 634426da, deny_unknown_fields;
schema mtg-kernel-line-a-panel-admission/v1, CLAUDE #483 and CODEX #570). It binds the bytes the shared
loader reads (the ExpandedModelSourceV1 file and the import descriptor it pins), the expected model
identity, the Legacy adapter flags as the smoke ran them (read from both seat models), and in
registry_pins the loaded nested identity's registry and namespace fields, which the collector compares
with the identity it loads. Routes are told apart by the receipt route plus the nested identity and
import-descriptor schemas, never by the evaluator's outer receipt schema.

An R14 receipt also carries acceptance {fable_section, codex_countersign, accepted_commit}. The producer
verifies each reference before writing (CODEX #570: syntax is not evidence; #573: affirmative and exact):
the Fable section heading matches exactly and its verdict countersigns; the Codex note holds exactly one
line with the acceptance label, 'R14 implementation acceptance: COUNTERSIGN <full accepted commit>'; the
commit exists, carries the pinned source card-DB constant, and is the commit the smoke evaluator was built
at. A Fable design countersign never supplies Codex's implementation acceptance. The section and note
bytes are hashed into evidence. A receipt never admits an unaccepted route. Receipts go to admission/r3/
(tool revision 3); admission/<member>.json (revision 1) and admission/v2/ (revision 2, a shape the
collector does not parse) are superseded.

Usage: python python/tools/line_a_panel_admission_v1.py LABEL
           [--fable-record PATH --fable-section 'FULL HEADING' --codex-note 'CODEX #N' --accepted-commit SHA]
"""
import argparse, hashlib, json, re, subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SEALED = Path('E:/mtg-line-a-panel-imports-20260927')
COLLAB = Path('C:/Users/Jack/IdeaProjects/collab')
SCHEMA = 'mtg-kernel-line-a-panel-admission/v1'
V3 = {'feature_contract_digest': '9319fbd41e6b42ec90d565c13f3b4f75a3898dafe3816387453c08419ba9cc68',
      'feature_encoding_digest': 'c4662291ca9a75525b51b51f3b5d512671340c05b69fd33fb0827c0c8af70a2b'}
R14_IMPORT_SCHEMA = 'mtg-kernel-frozen-play-registry-evolution-import/v1'
IDENTITY_SCHEMAS = {'strict': 'mtg-kernel-frozen-sideboard-play-transfer/v1',
                    'r14': 'mtg-kernel-frozen-sideboard-play-registry-evolution-transfer/v1'}
R14_SOURCE_CARD_DB_HASH = 'a06fa9566106f0ea'
R14_LOADER = 'mtg-kernel/src/sideboard_play_policy_v1/registry_evolution_v1.rs'
R14_PIN_LINE = 'const SOURCE_CARD_DB_HASH_V1: &str = "a06fa9566106f0ea";'
# Codex emits exactly one line with this label after implementation acceptance (CODEX #573).
ACCEPTANCE_LABEL = 'R14 implementation acceptance:'


def acceptance_line(commit): return ACCEPTANCE_LABEL + ' COUNTERSIGN ' + commit


def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fail(message): raise SystemExit('admission refused: ' + message)


def section(lines, start, prefixes):
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith(prefixes)), len(lines))
    return '\n'.join(lines[start:end]).rstrip() + '\n'


def fable_reference(record, heading):
    """The Fable section as 'FABLE-REVIEW-<date>.md#<heading>', its bytes hashed. The heading must equal the
    section's full heading exactly (one such section), and its verdict must countersign. A Fable countersign
    is the design verdict only; it never supplies Codex's implementation acceptance (codex_reference)."""
    record = Path(record)
    if not re.fullmatch(r'FABLE-REVIEW-\d{8}\.md', record.name):
        fail('the Fable record is not a FABLE-REVIEW-<date>.md file')
    lines = record.read_text(encoding='utf-8').splitlines()
    matches = [i for i, line in enumerate(lines) if line == '## ' + heading]
    if len(matches) != 1:
        fail('the Fable record has no single section with exactly the given heading')
    start = matches[0]
    title = lines[start][3:]
    verdict = re.search(r': (COUNTERSIGN|CHANGE-REQUIRED|REFUSE[D]?|REJECT(?:ED)?)\b', title)
    if not verdict or verdict.group(1) != 'COUNTERSIGN':
        fail('the Fable section verdict is not a countersign')
    body = section(lines, start, ('## ', '# '))
    return record.name + '#' + title, {'record': record.as_posix(), 'section_sha256': hashlib.sha256(body.encode('utf-8')).hexdigest()}


def codex_reference(note, commit, mailbox=None):
    """Codex's implementation acceptance (CODEX #573): the identified note (one heading '## CODEX #<n> [') has
    exactly one line containing ACCEPTANCE_LABEL, and that whole line is acceptance_line(accepted commit).
    A heading, a pending or negative line, or a commit mentioned in discussion is not acceptance."""
    if not re.fullmatch(r'CODEX #\d+', note):
        fail('the Codex note is not written CODEX #<n>')
    mailbox = Path(mailbox) if mailbox else COLLAB / 'TO-CODEX.md'
    lines = mailbox.read_text(encoding='utf-8').splitlines()
    starts = [i for i, line in enumerate(lines) if line.startswith('## ' + note + ' [')]
    if len(starts) != 1:
        fail(note + ' is not a single note in ' + mailbox.name)
    start = starts[0]
    body = section(lines, start, ('## ',))
    claimed = [line for line in body.splitlines() if ACCEPTANCE_LABEL in line]
    if claimed != [acceptance_line(commit)]:
        fail(note + ' holds no single exact line ' + repr(acceptance_line('<full accepted commit>')))
    return {'mailbox': mailbox.as_posix(), 'heading': lines[start][3:], 'note_sha256': hashlib.sha256(body.encode('utf-8')).hexdigest()}


def commit_reference(commit):
    """The accepted commit exists here and carries the pinned source card-DB constant (R14 verdict change 1)."""
    if not re.fullmatch(r'[0-9a-f]{40}', commit):
        fail('the accepted commit is not 40 lowercase hex')
    if subprocess.run(['git', '-C', str(REPO), 'cat-file', '-e', commit + '^{commit}'], capture_output=True).returncode:
        fail('the accepted commit is not in this repository')
    loader = subprocess.run(['git', '-C', str(REPO), 'show', commit + ':' + R14_LOADER], capture_output=True).stdout.decode('utf-8', 'replace')
    if R14_PIN_LINE not in loader:
        fail('the accepted commit does not carry the pinned source card-DB hash')
    return {'repository': REPO.as_posix(), 'loader_path': R14_LOADER, 'pinned_source_card_db_line': R14_PIN_LINE}


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
    if (nested['source_registry_sha256'], nested['destination_card_db_hash']) != (
            descriptor['expected_source_registry_sha256'], descriptor['expected_destination_card_db_hash']):
        fail('loaded identity and descriptor disagree on the registry pins')
    seats = smoke.get('models') or []
    if len(seats) != 2:
        fail('smoke receipt lacks both seat models')
    if {seat['weights_sha256'] for seat in seats} != {identity['model']['weights_sha256']}:
        fail('smoke seats and round-trip identity disagree on weights')
    flags = [(seat.get('v3_forced_actions'), seat.get('v3_spell_target_reference_adapter')) for seat in seats]
    if len(set(flags)) != 1 or flags[0] != (True, True):
        fail('smoke did not run both seats with the Legacy V3 adapter flags true')
    if {(seat.get('identity') or {}).get('source_import', {}).get('schema') for seat in seats} != {IDENTITY_SCHEMAS[route]}:
        fail('smoke seat identities do not carry the route identity schema')
    acceptance_given = any((args.fable_record, args.fable_section, args.codex_note, args.accepted_commit))
    acceptance = verification = None
    if route == 'strict' and acceptance_given:
        fail('a strict receipt carries no acceptance reference')
    if route == 'r14':
        if not (args.fable_record and args.fable_section and args.codex_note and args.accepted_commit):
            fail('an R14 receipt requires the Fable section, the Codex implementation note and the accepted commit')
        if nested.get('source_card_db_hash') != R14_SOURCE_CARD_DB_HASH:
            fail('source card-DB hash is not the pinned R14 source')
        if descriptor['expected_allowlist_sha256'] not in nested.get('namespace_rule', ''):
            fail('namespace rule does not name the pinned allowlist')
        if smoke.get('evaluator_git_head') != args.accepted_commit:
            fail('the smoke evaluator was not built at the accepted commit')
        fable_section, fable_evidence = fable_reference(args.fable_record, args.fable_section)
        acceptance = {'fable_section': fable_section, 'codex_countersign': args.codex_note, 'accepted_commit': args.accepted_commit}
        verification = {'fable': fable_evidence, 'codex': codex_reference(args.codex_note, args.accepted_commit),
                        'commit': commit_reference(args.accepted_commit)}
    receipt = {
        'schema': SCHEMA,
        'member': args.label,
        'route': route,
        'model_source': {'path': model_source_path.as_posix(), 'sha256': sha(model_source_path)},
        'import_descriptor': {'path': import_path.as_posix(), 'sha256': sha(import_path), 'schema': schema},
        'expected_identity': {'identity_schema': nested['schema'],
                              'model_parameter_sha256': identity['model']['model_parameter_sha256'],
                              'weights_sha256': identity['model']['weights_sha256'],
                              'feature_generation': 'V3', 'observation_successor': True, **V3},
        'adapter_flags': {'v3_forced_actions': flags[0][0], 'v3_spell_target_reference_adapter': flags[0][1]},
        'registry_pins': {'source_registry_sha256': nested['source_registry_sha256'],
                          'source_card_db_hash': nested['source_card_db_hash'],
                          'namespace_rule': nested['namespace_rule'],
                          'destination_registry_sha256': nested['destination_registry_sha256'],
                          'destination_card_db_hash': nested['destination_card_db_hash'],
                          'allowlist_sha256': descriptor.get('expected_allowlist_sha256')},
        'evidence': {'roundtrip_receipt': {'path': (receipts / 'roundtrip.json').as_posix(), 'sha256': sha(receipts / 'roundtrip.json')},
                     'smoke_receipt': {'path': (receipts / 'smoke.json').as_posix(), 'sha256': sha(receipts / 'smoke.json')},
                     'exporter_build_commit': descriptor['source_registry_git_commit'],
                     'evaluator_sha256': smoke['evaluator_sha256'], 'evaluator_git_head': smoke['evaluator_git_head'],
                     'adapter_flags_source': 'smoke receipt seat models',
                     'registry_pins_source': 'loaded nested identity (round-trip receipt), equal to the descriptor pins',
                     'acceptance_verification': verification},
        'nonclaims': ['engineering admission of a route-accepted import; not a strength claim and not roster authority',
                      'no outcome, winner or score was read',
                      'routes are told apart by route plus the nested identity and import-descriptor schemas, never by the evaluator outer receipt schema'],
    }
    if acceptance:
        receipt['acceptance'] = acceptance
    data = (json.dumps(receipt, indent=2) + '\n').encode('utf-8')
    path = SEALED / 'admission' / 'r3' / (name + '.json')
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        fail('refusing to overwrite ' + path.as_posix())
    path.write_bytes(data)
    print(path.as_posix(), hashlib.sha256(data).hexdigest(), route)


if __name__ == '__main__':
    main()
