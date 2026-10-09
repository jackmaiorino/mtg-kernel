import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import Mock, patch


SOURCE = Path(__file__).resolve().parents[2] / 'docs/reports/training_speedups_20261009/postprocess_case.py'
SPEC = importlib.util.spec_from_file_location('speedup_postprocess', SOURCE)
POST = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POST)


class RecoveryBindingTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.native = self.root / 'matched-native'
        self.native.mkdir()
        (self.native / 'nested').mkdir()
        (self.native / 'completion.json').write_text('{"case":"old"}', encoding='utf-8')
        (self.native / 'nested/update.json').write_text('{"adam_step":162}', encoding='utf-8')
        self.archive = {'shards': [
            {'files': {'completion.json': POST.pin(self.native / 'completion.json')['sha256']}},
            {'files': {'nested/update.json': POST.pin(self.native / 'nested/update.json')['sha256']}},
        ]}
        self.archive_pin = self.save('archive.json', self.archive)

    def save(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data), encoding='utf-8')
        return POST.pin(path)

    def test_exact_inventory_and_bytes_accept(self):
        POST.verify_native_recovery(self.native, self.archive)

    def test_replaced_bytes_refuse_without_changing_raw(self):
        path = self.native / 'completion.json'
        changed = b'{"case":"new"}'
        path.write_bytes(changed)
        with self.assertRaisesRegex(ValueError, 'native recovery bytes differ'):
            POST.verify_native_recovery(self.native, self.archive)
        self.assertEqual(path.read_bytes(), changed)

    def test_missing_or_extra_members_refuse(self):
        extra = self.native / 'new-case.json'
        extra.write_text('{}', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'inventory differs'):
            POST.verify_native_recovery(self.native, self.archive)
        extra.unlink()
        (self.native / 'nested/update.json').unlink()
        with self.assertRaisesRegex(ValueError, 'inventory differs'):
            POST.verify_native_recovery(self.native, self.archive)

    def test_duplicate_archive_members_refuse(self):
        self.archive['shards'][1]['files']['completion.json'] = self.archive['shards'][0]['files']['completion.json']
        with self.assertRaisesRegex(ValueError, 'duplicate recovery member'):
            POST.verify_native_recovery(self.native, self.archive)

    def input_tuple(self, out):
        return (self.root, {'path': 'request', 'sha256': 'request-sha'}, {}, self.root, self.root,
                self.native, {'path': 'report', 'sha256': 'report-sha'}, {'archive': self.archive_pin}, out)

    def test_inspect_refuses_stale_case_before_creating_output(self):
        out = self.root / 'inspection'
        (self.native / 'completion.json').write_text('{"case":"new"}', encoding='utf-8')
        with patch.object(POST, 'inputs', return_value=self.input_tuple(out)), \
                patch.object(POST.importlib, 'import_module') as imported:
            with self.assertRaisesRegex(ValueError, 'native recovery bytes differ'):
                POST.inspect(SimpleNamespace())
        imported.assert_not_called()
        self.assertFalse(out.exists())
        self.assertTrue(self.native.exists())

    def test_retain_rechecks_raw_after_valid_case_and_copy_receipts(self):
        out = self.root / 'inspection'
        out.mkdir()
        tools = self.root / 'launcher/python/tools'
        tools.mkdir(parents=True)
        retainer = tools / 'nine_deck_campaign_v1.py'
        retainer.write_text('# mocked retainer; never invoked', encoding='utf-8')
        request_pin, report_pin = self.input_tuple(out)[1], self.input_tuple(out)[6]
        plan_pin = self.save('plan.json', {'source_host': 'HALEYSPC', 'destination_host': 'DESKTOP-DJ1C40R', 'files': []})
        receipt_pin = self.save('copy.json', {'schema': 'training-speedup-independent-cold-copy/v1',
            'complete': True, 'plan': plan_pin, 'source_host': 'HALEYSPC',
            'destination_host': 'DESKTOP-DJ1C40R', 'files': []})
        inspection = {'complete': True, 'request': request_pin, 'report': report_pin,
            'native_root': str(self.native), 'recovery_copy_plan': plan_pin,
            'maintenance_sources': {'retainer': POST.pin(retainer)}}
        for key in ('decks', 't1', 'exposure', 'final_checkpoint', 'embedding_gate'):
            inspection[key] = self.save(key + '.json', {})
        self.save('inspection/inspect.json', inspection)
        args = SimpleNamespace(cold_copy_receipt=receipt_pin['path'], cold_copy_sha256=receipt_pin['sha256'],
                               launcher_root=self.root / 'launcher')
        campaign = SimpleNamespace(retain_block=Mock())
        original_pin = POST.pin
        def replace_raw_after_receipt_checks(path):
            result = original_pin(path)
            if Path(path) == retainer:
                (self.native / 'completion.json').write_text('{"case":"new"}', encoding='utf-8')
            return result
        with patch.object(POST, 'inputs', return_value=self.input_tuple(out)), \
                patch.object(POST.platform, 'node', return_value='HALEYSPC'), \
                patch.object(POST.importlib, 'import_module', return_value=campaign), \
                patch.object(POST, 'pin', side_effect=replace_raw_after_receipt_checks):
            with self.assertRaisesRegex(ValueError, 'native recovery bytes differ'):
                POST.retain(args)
        campaign.retain_block.assert_not_called()
        self.assertTrue(self.native.exists())
        self.assertFalse((out / 'retained').exists())
        self.assertFalse((out / 'retain.json').exists())


if __name__ == '__main__':
    unittest.main()
