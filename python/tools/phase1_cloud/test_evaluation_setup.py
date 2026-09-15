"""Offline reproduction of native create_dir parent requirements. No engine."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from common import pin, write
from cloud_worker import prepare_evaluation_output


class EvaluationSetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory();self.hot=Path(self.temporary.name).resolve()
        self.output=self.hot/'evaluation'/'replay'
        self.config=self.hot/'config.json';write(self.config,{'output_directory':str(self.output)})
        self.reference=pin(self.config)
    def tearDown(self):self.temporary.cleanup()

    def test_missing_parent_is_created_but_native_output_stays_fresh(self):
        self.assertFalse(self.output.parent.exists())
        result=prepare_evaluation_output(self.hot,'replay',self.reference)
        self.assertEqual(result,self.output);self.assertTrue(result.parent.is_dir())
        self.assertFalse(result.exists())
        # This is the filesystem operation used by native create_dir, not an
        # engine launch. It previously failed because evaluation/ was absent.
        result.mkdir()
        self.assertTrue(result.is_dir())

    def test_existing_parent_preserves_other_completed_evaluations(self):
        previous=self.output.parent/'earlier';previous.mkdir(parents=True)
        (previous/'completion.json').write_text('preserve old evidence')
        prepare_evaluation_output(self.hot,'replay',self.reference)
        self.assertEqual((previous/'completion.json').read_text(),'preserve old evidence')
        self.assertFalse(self.output.exists())

    def test_existing_output_is_rejected_without_changes(self):
        self.output.mkdir(parents=True);marker=self.output/'completion.json';marker.write_text('old')
        with self.assertRaisesRegex(ValueError,'already exists'):
            prepare_evaluation_output(self.hot,'replay',self.reference)
        self.assertEqual(marker.read_text(),'old')

    def test_misdirected_config_and_name_are_rejected_before_directory_creation(self):
        with self.assertRaisesRegex(ValueError,'invalid packaged evaluation name'):
            prepare_evaluation_output(self.hot,'../outside',self.reference)
        write(self.config,{'output_directory':str(self.hot/'outside')},replace=True)
        with self.assertRaisesRegex(ValueError,'exact packaged hot root'):
            prepare_evaluation_output(self.hot,'replay',pin(self.config))
        self.assertFalse(self.output.parent.exists())

    def test_changed_config_pin_is_rejected_before_directory_creation(self):
        write(self.config,{'output_directory':str(self.output),'changed':True},replace=True)
        with self.assertRaisesRegex(ValueError,'SHA256 differs'):
            prepare_evaluation_output(self.hot,'replay',self.reference)
        self.assertFalse(self.output.parent.exists())

    def test_parent_file_or_link_is_rejected(self):
        self.output.parent.write_text('not a directory')
        with self.assertRaisesRegex(ValueError,'ordinary hot directory'):
            prepare_evaluation_output(self.hot,'replay',self.reference)
        self.assertEqual(self.output.parent.read_text(),'not a directory')
        # A mock covers symlink refusal without requiring Windows link privilege.
        original_is_symlink=Path.is_symlink
        def is_symlink(path):return path==self.output.parent or original_is_symlink(path)
        with patch.object(Path,'is_symlink',is_symlink):
            with self.assertRaisesRegex(ValueError,'ordinary hot directory'):
                prepare_evaluation_output(self.hot,'replay',self.reference)


if __name__=='__main__':unittest.main(verbosity=2)
