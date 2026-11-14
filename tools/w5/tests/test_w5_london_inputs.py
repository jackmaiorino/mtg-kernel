"""The arm M inputs (tools/w5/london_base.py and tools/w5/v7_dose.py), on synthetic bases and dose records shaped like
chain A's: every episode gets the London opening, modes follow the opponent kind and the learner seat, the decision
caps rise by 70 exactly once, a converted base is refused, the hash file matches the content, and the FlatV7 transfer
changes nothing but its two digests (checkpoint and play-import pins byte-identical)."""
import contextlib
import copy
import hashlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import london_base  # noqa: E402
import v7_dose  # noqa: E402

V4 = v7_dose.V4
V7 = v7_dose.V7


def pin(name, digit):
    return {'path': f'/srv/work/inputs/objects/{name}.json', 'sha256': digit * 64}


def source(name, digit, import_name, transfer=V4):
    return {'checkpoint': pin(name, digit), 'feature_transfer': dict(transfer),
            'play_import': pin(import_name, '9')}


def deck(label):
    return {'label': label, 'mainboard': [1, 1, 2, 2], 'sideboard': [3, 3]}


def episode(ident, seat):
    return {'id': ident, 'seed': 1484665670765204036 + seat, 'starting_player': seat, 'learner_seat': seat,
            'registered': [deck('Spy/4d47'), deck('Elves/0643')], 'selected': [deck('Spy/4d47'), deck('Elves/0643')],
            'postboard': False, 'max_physical_decisions': 100000, 'max_policy_steps': 200000,
            'runtime_rules': 'resolution-boundary-v1'}


def make_base():
    """3 iterations x 4 episodes; (learner seat, opponent) takes all six combinations over the base."""
    kinds = [{'kind': 'current'}, {'kind': 'fixed', 'id': 'parent'}, {'kind': 'fixed', 'id': 'fresh-a-block48-end'}]
    iterations, n = [], 0
    for i in range(3):
        entries = []
        for j in range(4):
            entries.append({'episode': episode(f'e{n}', (i + j) % 2), 'opponent': dict(kinds[(i + j) % 3])})
            n += 1
        iterations.append({'episodes': entries})
    return {'schema': london_base.SCHEMA, 'initial_source': source('d6107922', 'd', 't1'),
            'opponents': [{'id': 'fresh-a-block48-end', 'source': source('beb86b41', 'b', 'a48')},
                          {'id': 'parent', 'source': source('d6107922', 'd', 't1')}],
            'iterations': iterations, 'learning_rate': 0.0001, 'value_coefficient': 0.5,
            'loss_selection': {'kind': 'gae_advantage_value_v1', 'gamma': 1.0, 'lambda': 0.9,
                               'entropy_coefficient': 0.0},
            'max_non_natural_episode_fraction': 0.2, 'update_backend': {'kind': 'cpu'}, 'collection_workers': 1,
            'preparation_workers': 2, 'output_directory': 'unused'}


def make_dose(transfer=V4):
    return {'origin': {'after_state_sha256': 'a' * 64, 'boundary': 1048576, 'chain': 'c1'},
            'source': source('3bf48574', '3', 't1-import', transfer)}


def entries(base):
    return [e for it in base['iterations'] for e in it['episodes']]


def expected_modes(entry):
    if entry['opponent']['kind'] == 'current':
        return ['learned', 'learned']
    return ['learned', 'keep_seven'] if entry['episode']['learner_seat'] == 0 else ['keep_seven', 'learned']


def revert(doc, base):
    """A converted base with the new fields removed and the caps put back to `base`'s: the unconverted base."""
    out = copy.deepcopy(doc)
    for e, old in zip(entries(out), entries(base)):
        for key in ('opening', 'mulligan_modes'):
            del e['episode'][key]
        for key in london_base.CAP_KEYS:
            e['episode'][key] = old['episode'][key]
    return out


class LondonBaseTests(unittest.TestCase):
    def test_every_episode_gets_the_opening_and_modes_by_opponent_and_seat(self):
        base = make_base()
        doc, summary = london_base.build(base)
        self.assertEqual(len(entries(doc)), 12)
        combos = set()
        for old, new in zip(entries(base), entries(doc)):
            ep = new['episode']
            self.assertEqual(ep['opening'], 'london-v1')
            self.assertEqual(ep['mulligan_modes'], expected_modes(old))
            self.assertEqual(ep['mulligan_modes'][ep['learner_seat']], 'learned')
            combos.add((old['opponent']['kind'], old['opponent'].get('id'), ep['learner_seat']))
        self.assertEqual(len(combos), 6)  # self-play and both fixed opponents, each with both learner seats
        self.assertEqual(summary['episodes'], 12)
        self.assertEqual(summary['by_opponent'], {'current': 4, 'fixed:parent': 4, 'fixed:fresh-a-block48-end': 4})
        counts = {}
        for old in entries(base):
            key = '/'.join(expected_modes(old))
            counts[key] = counts.get(key, 0) + 1
        self.assertEqual(sorted(counts), ['keep_seven/learned', 'learned/keep_seven', 'learned/learned'])
        self.assertEqual(summary['by_modes'], counts)

    def test_the_new_fields_follow_the_existing_ones_and_the_caps_rise_by_70_within_the_trainer_limits(self):
        base = make_base()
        doc, summary = london_base.build(base)
        for e in entries(doc):
            ep = e['episode']
            self.assertEqual(list(ep)[-3:], ['runtime_rules', 'opening', 'mulligan_modes'])
            # The trainer refuses max_physical_decisions above 100000, where chain A's cap already is.
            self.assertEqual(ep['max_physical_decisions'], 100000)
            self.assertEqual(ep['max_policy_steps'], 200070)
            self.assertLessEqual(ep['max_physical_decisions'], london_base.CAP_CEILING['max_physical_decisions'])
            self.assertLessEqual(ep['max_policy_steps'], london_base.CAP_CEILING['max_policy_steps'])
        self.assertEqual(summary['caps']['max_physical_decisions'], {'before': [100000], 'after': [100000]})
        self.assertEqual(summary['caps']['max_policy_steps'], {'before': [200000], 'after': [200070]})
        self.assertEqual(summary['capped_at_trainer_limit'], {'max_physical_decisions': 12})

    def test_a_cap_never_leaves_the_trainer_range_and_a_base_above_it_is_refused(self):
        self.assertEqual(london_base.CAP_CEILING, {'max_physical_decisions': 100000, 'max_policy_steps': 1000000})
        base = make_base()
        entries(base)[0]['episode']['max_physical_decisions'] = 99950  # 70 would pass the limit: cut at it
        entries(base)[1]['episode']['max_physical_decisions'] = 99930  # exactly the limit
        entries(base)[2]['episode']['max_policy_steps'] = 999990
        doc, summary = london_base.build(base)
        got = [e['episode']['max_physical_decisions'] for e in entries(doc)[:3]]
        self.assertEqual(got, [100000, 100000, 100000])
        self.assertEqual(entries(doc)[2]['episode']['max_policy_steps'], 1000000)
        self.assertEqual(summary['capped_at_trainer_limit'], {'max_physical_decisions': 11, 'max_policy_steps': 1})
        for key, bad in (('max_physical_decisions', 100001), ('max_policy_steps', 1000001)):
            base = make_base()
            entries(base)[4]['episode'][key] = bad
            with self.assertRaisesRegex(london_base.LondonBaseError, 'above the trainer limit'):
                london_base.build(base)
        base = make_base()  # nothing is cut: no capped entry in the summary
        for e in entries(base):
            e['episode']['max_physical_decisions'] = 5000
        self.assertNotIn('capped_at_trainer_limit', london_base.build(base)[1])

    def test_nothing_else_changes_and_the_input_is_not_modified(self):
        base = make_base()
        before = copy.deepcopy(base)
        doc, _ = london_base.build(base)
        self.assertEqual(base, before)
        self.assertEqual(revert(doc, base), base)
        self.assertEqual(list(doc), list(base))
        for key in ('initial_source', 'opponents', 'learning_rate', 'loss_selection', 'output_directory'):
            self.assertEqual(doc[key], base[key])
        self.assertEqual(json.dumps(revert(doc, base), indent=1), json.dumps(base, indent=1))  # same key order too

    def test_different_caps_are_each_raised_by_70(self):
        base = make_base()
        entries(base)[0]['episode']['max_physical_decisions'] = 5
        entries(base)[0]['episode']['max_policy_steps'] = 6
        doc, summary = london_base.build(base)
        self.assertEqual(entries(doc)[0]['episode']['max_physical_decisions'], 75)
        self.assertEqual(entries(doc)[0]['episode']['max_policy_steps'], 76)
        self.assertEqual(summary['caps']['max_physical_decisions']['after'], [75, 100000])

    def test_a_converted_base_is_refused_so_the_caps_rise_once(self):
        doc, _ = london_base.build(make_base())
        with self.assertRaisesRegex(london_base.LondonBaseError, "already has 'opening'"):
            london_base.build(doc)
        for key, value in (('mulligan_modes', ['learned', 'learned']), ('london_keep_offset', 1.5),
                           ('reset', {}), ('opponent_search', {})):
            base = make_base()
            entries(base)[5]['episode'][key] = value
            with self.assertRaisesRegex(london_base.LondonBaseError, key):
                london_base.build(base)

    def test_unknown_opponents_seats_caps_and_shapes_are_refused(self):
        def refused(mutate, text):
            base = make_base()
            mutate(base)
            with self.assertRaisesRegex(london_base.LondonBaseError, text):
                london_base.build(base)
        refused(lambda b: entries(b)[2]['opponent'].update(kind='completed_iteration'), 'unknown opponent kind')
        refused(lambda b: entries(b)[2].pop('opponent'), 'no opponent entry')
        refused(lambda b: entries(b)[0]['opponent'].update(id='parent'), 'carries only its kind')
        refused(lambda b: entries(b)[1]['opponent'].update(extra=1), 'exactly its kind and id')
        refused(lambda b: entries(b)[1]['opponent'].update(id='nobody'), 'not declared')
        refused(lambda b: entries(b)[3]['episode'].update(learner_seat=2), 'learner_seat 2')
        refused(lambda b: entries(b)[3]['episode'].update(learner_seat=True), 'learner_seat True')
        refused(lambda b: entries(b)[3]['episode'].pop('learner_seat'), 'learner_seat None')
        refused(lambda b: entries(b)[4]['episode'].pop('max_policy_steps'), 'max_policy_steps')
        refused(lambda b: entries(b)[4]['episode'].update(max_physical_decisions=1.5), 'max_physical_decisions')
        refused(lambda b: entries(b)[4]['episode'].update(max_physical_decisions='100'), 'max_physical_decisions')
        refused(lambda b: b.update(schema='something-else'), 'not a mtg-kernel')
        refused(lambda b: b.update(iterations=[]), 'no iterations')
        refused(lambda b: b['iterations'][1].update(episodes=[]), 'iteration 1 has no episodes')
        refused(lambda b: b['opponents'].append({'id': 'parent', 'source': {}}), 'declared twice')

    def test_output_bytes_are_stable_and_the_hash_file_matches(self):
        doc, _ = london_base.build(make_base())
        again, _ = london_base.build(make_base())
        data = london_base.render(doc)
        self.assertEqual(data, london_base.render(again))
        text = data.decode('utf-8')
        self.assertEqual(text, json.dumps(doc, indent=1) + '\n')
        self.assertTrue(text.endswith('}\n') and '\r' not in text)
        self.assertEqual(list(json.loads(text)), list(doc))  # the base's own key order survives the file
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / 'sub' / 'london-base.json'
            digest = london_base.write_with_hash(doc, out)
            self.assertEqual(out.read_bytes(), data)
            self.assertEqual(digest, hashlib.sha256(data).hexdigest())
            self.assertEqual(out.with_name('london-base.json.sha256').read_text(), f'{digest}  london-base.json\n')
            self.assertEqual(london_base.write_with_hash(doc, out), digest)  # the same bytes again: allowed
            other = copy.deepcopy(doc)
            other['learning_rate'] = 0.5
            with self.assertRaisesRegex(london_base.LondonBaseError, 'never overwritten'):
                london_base.write_with_hash(other, out)
            self.assertEqual(out.read_bytes(), data)

    def test_cli_writes_and_check_writes_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            src, out = Path(tmp) / 'base.json', Path(tmp) / 'out' / 'london.json'
            src.write_text(json.dumps(make_base(), indent=1) + '\r\n', encoding='utf-8', newline='')  # CRLF input
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                london_base.main(['--base', str(src), '--out', str(out), '--check'])
            checked = json.loads(buf.getvalue())
            self.assertFalse(out.exists() or out.parent.exists())
            self.assertFalse(checked['written'])
            self.assertEqual(checked['input_sha256'], hashlib.sha256(src.read_bytes()).hexdigest())
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                london_base.main(['--base', str(src), '--out', str(out)])
            wrote = json.loads(buf.getvalue())
            self.assertTrue(wrote['written'])
            self.assertEqual(wrote['output_sha256'], checked['output_sha256'])
            self.assertEqual(wrote['output_sha256'], hashlib.sha256(out.read_bytes()).hexdigest())
            self.assertIn(wrote['output_sha256'], out.with_name('london.json.sha256').read_text())
            with self.assertRaises(SystemExit) as raised:  # the converted base is refused as an input
                london_base.main(['--base', str(out), '--out', str(Path(tmp) / 'twice.json')])
            self.assertIn('refused', str(raised.exception))
            self.assertFalse((Path(tmp) / 'twice.json').exists())


class V7DoseTests(unittest.TestCase):
    def test_a_dose_carries_the_transfer_and_nothing_else_changes(self):
        dose = make_dose()
        before = copy.deepcopy(dose)
        out = v7_dose.convert_dose(dose)
        self.assertEqual(dose, before)
        self.assertEqual(out['source']['feature_transfer'], V7)
        self.assertEqual(out['source']['checkpoint'], dose['source']['checkpoint'])
        self.assertEqual(out['source']['play_import'], dose['source']['play_import'])
        self.assertEqual(out['origin'], dose['origin'])
        self.assertEqual(list(out), list(dose))
        self.assertEqual(list(out['source']), list(dose['source']))
        restored = copy.deepcopy(out)
        restored['source']['feature_transfer'] = dict(V4)
        self.assertEqual(restored, dose)

    def test_the_bytes_differ_from_the_input_in_the_two_digests_only(self):
        dose = make_dose()
        text = json.dumps(dose, indent=1) + '\n'
        out = v7_dose.render(v7_dose.convert_dose(dose)).decode('utf-8')
        expected = text.replace(V4['expected_feature_contract_digest'], V7['expected_feature_contract_digest']) \
            .replace(V4['expected_feature_encoding_digest'], V7['expected_feature_encoding_digest'])
        self.assertEqual(out, expected)
        self.assertNotEqual(out, text)

    def test_the_transfer_keeps_the_inputs_own_key_order(self):
        reversed_v4 = dict(reversed(list(V4.items())))  # encoding digest first
        dose = make_dose(reversed_v4)
        self.assertEqual(list(dose['source']['feature_transfer']), list(reversed_v4))
        out = v7_dose.convert_dose(dose)
        self.assertEqual(list(out['source']['feature_transfer']), list(reversed_v4))
        self.assertEqual(out['source']['feature_transfer'], V7)
        # the same text as the input with the two digests replaced, so no key moved
        text = json.dumps(dose, indent=1) + '\n'
        expected = text.replace(V4['expected_feature_contract_digest'], V7['expected_feature_contract_digest']) \
            .replace(V4['expected_feature_encoding_digest'], V7['expected_feature_encoding_digest'])
        self.assertEqual(v7_dose.render(out).decode('utf-8'), expected)
        # an already-V7 source in either order converts to the same bytes
        reversed_v7 = dict(reversed(list(V7.items())))
        again = make_dose(reversed_v7)
        self.assertEqual(v7_dose.render(v7_dose.convert_dose(again)), v7_dose.render(again))
        base = make_base()
        for item in base['opponents']:
            item['source']['feature_transfer'] = dict(reversed_v4)
        converted, _ = v7_dose.convert_base(base)
        for item in converted['opponents']:
            self.assertEqual(list(item['source']['feature_transfer']), list(reversed_v4))
            self.assertEqual(item['source']['feature_transfer'], V7)

    def test_the_v7_pair_converts_to_itself_and_other_transfers_are_refused(self):
        v7_dose_record = make_dose(V7)
        self.assertEqual(v7_dose.convert_dose(v7_dose_record), v7_dose_record)
        other = {'expected_feature_contract_digest': 'd' * 64, 'expected_feature_encoding_digest': 'e' * 64}
        with self.assertRaisesRegex(v7_dose.DoseError, 'different transfer'):
            v7_dose.convert_dose(make_dose(other))
        half = dict(V7, expected_feature_encoding_digest=V4['expected_feature_encoding_digest'])
        with self.assertRaisesRegex(v7_dose.DoseError, 'different transfer'):
            v7_dose.convert_dose(make_dose(half))
        half = dict(V4, expected_feature_contract_digest=V7['expected_feature_contract_digest'])
        with self.assertRaisesRegex(v7_dose.DoseError, 'different transfer'):
            v7_dose.convert_dose(make_dose(half))
        extra = dict(V4, note='x')
        with self.assertRaisesRegex(v7_dose.DoseError, 'feature_transfer has keys'):
            v7_dose.convert_dose(make_dose(extra))
        missing = make_dose()
        del missing['source']['feature_transfer']
        with self.assertRaisesRegex(v7_dose.DoseError, 'declares no feature_transfer'):
            v7_dose.convert_dose(missing)
        with self.assertRaisesRegex(v7_dose.DoseError, 'no source'):
            v7_dose.convert_dose({'origin': {}})
        with self.assertRaisesRegex(v7_dose.DoseError, 'not a model source'):
            v7_dose.convert_dose({'source': []})

    def test_a_base_gets_the_transfer_on_its_named_opponents_only(self):
        base = make_base()
        before = copy.deepcopy(base)
        out, ids = v7_dose.convert_base(base)
        self.assertEqual(base, before)
        self.assertEqual(ids, ['fresh-a-block48-end', 'parent'])
        for old, new in zip(base['opponents'], out['opponents']):
            self.assertEqual(new['id'], old['id'])
            self.assertEqual(new['source']['feature_transfer'], V7)
            self.assertEqual(new['source']['checkpoint'], old['source']['checkpoint'])
            self.assertEqual(new['source']['play_import'], old['source']['play_import'])
        self.assertEqual(out['initial_source'], base['initial_source'])  # replaced by the dose in every segment
        self.assertEqual(out['iterations'], base['iterations'])
        self.assertEqual(list(out), list(base))
        restored = copy.deepcopy(out)
        for item in restored['opponents']:
            item['source']['feature_transfer'] = dict(V4)
        self.assertEqual(restored, base)

    def test_a_base_without_opponents_or_with_a_foreign_transfer_is_refused(self):
        base = make_base()
        base['opponents'] = []
        with self.assertRaisesRegex(v7_dose.DoseError, 'no named opponents'):
            v7_dose.convert_base(base)
        base = make_base()
        base['opponents'][1]['source']['feature_transfer'] = {
            'expected_feature_contract_digest': 'f' * 64, 'expected_feature_encoding_digest': V4[
                'expected_feature_encoding_digest']}
        with self.assertRaisesRegex(v7_dose.DoseError, r"opponents\[1\] \('parent'\).*different transfer"):
            v7_dose.convert_base(base)
        base = make_base()
        del base['opponents'][0]['source']
        with self.assertRaisesRegex(v7_dose.DoseError, 'no source'):
            v7_dose.convert_base(base)

    def test_the_two_conversions_commute(self):
        base = make_base()
        a, _ = london_base.build(v7_dose.convert_base(base)[0])
        b, _ = v7_dose.convert_base(london_base.build(base)[0])
        self.assertEqual(v7_dose.render(a), v7_dose.render(b))
        self.assertEqual(london_base.render(a), v7_dose.render(b))  # one format

    def test_cli_dose_and_base_with_check_and_hash_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            dose, base = tmp / 'dose.json', tmp / 'base.json'
            dose.write_text(json.dumps(make_dose(), indent=1, sort_keys=True), encoding='utf-8')  # no final newline
            base.write_text(json.dumps(make_base(), indent=1) + '\n', encoding='utf-8')
            cases = (('dose', dose, tmp / 'o1' / 'dose-v7.json'), ('base', base, tmp / 'o2' / 'base-v7.json'))
            for cmd, src, out in cases:
                flag = '--dose' if cmd == 'dose' else '--base'
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf):
                    v7_dose.main([cmd, flag, str(src), '--out', str(out), '--check'])
                self.assertFalse(out.parent.exists())
                checked = json.loads(buf.getvalue())
                self.assertEqual(checked['transfer'], V7)
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf):
                    v7_dose.main([cmd, flag, str(src), '--out', str(out)])
                wrote = json.loads(buf.getvalue())
                self.assertTrue(wrote['written'])
                self.assertEqual(wrote['output_sha256'], checked['output_sha256'])
                digest = hashlib.sha256(out.read_bytes()).hexdigest()
                self.assertEqual(digest, wrote['output_sha256'])
                self.assertEqual(out.with_name(out.name + '.sha256').read_text(), f'{digest}  {out.name}\n')
                with contextlib.redirect_stdout(io.StringIO()):  # a converted file converts to itself, same bytes
                    v7_dose.main([cmd, flag, str(out), '--out', str(out)])
                self.assertEqual(hashlib.sha256(out.read_bytes()).hexdigest(), digest)
            bad = tmp / 'bad.json'
            bad.write_text(json.dumps(make_dose({'expected_feature_contract_digest': '1' * 64,
                                                 'expected_feature_encoding_digest': '2' * 64})), encoding='utf-8')
            with self.assertRaises(SystemExit) as raised:
                v7_dose.main(['dose', '--dose', str(bad), '--out', str(tmp / 'bad-out.json')])
            self.assertIn('refused', str(raised.exception))
            self.assertFalse((tmp / 'bad-out.json').exists())


if __name__ == '__main__':
    unittest.main()
