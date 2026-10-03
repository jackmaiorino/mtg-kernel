"""Offline rejection of ambiguous model relocation before package publication."""
import copy
import os
from pathlib import Path
import tempfile
import unittest
from common import pin, read, write
from pack import prepare
import test_cloud


class RelocationPreflightTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory();self.root=Path(self.temporary.name)
        self.spec=test_cloud.PackagingTests().fixture(self.root)
        self.source=read(self.spec['training_config']['path'])['initial_source']
    def tearDown(self):self.temporary.cleanup()

    def evaluation(self,source):
        value={'mode':'run_population_batch','model_sources':[source,self.source],
            'policies':[{'kind':'keep'}]*2,'matches':[{'seed':99}],'output_directory':'old'}
        write(self.root/'evaluation.json',value)
        self.spec['evaluation_configs']={'replay':pin(self.root/'evaluation.json')}

    def assert_unpublished(self):
        for name in ('payload/manifest.json','payload.tar','package.json'):
            self.assertFalse((self.root/'package'/name).exists(),name)

    def test_equal_bytes_from_unequal_original_references_reject_before_publication(self):
        other=copy.deepcopy(self.source)
        original=Path(other['checkpoint']['path']);alias=self.root/'checkpoint-copy.json'
        alias.write_bytes(original.read_bytes());other['checkpoint']=pin(alias)
        self.assertEqual(other['checkpoint']['sha256'],self.source['checkpoint']['sha256'])
        self.evaluation(other)
        with self.assertRaisesRegex(ValueError,'non-injective model relocation'):
            prepare(self.spec,self.root/'package')
        self.assert_unpublished()

    @unittest.skipUnless(os.name=='nt','Windows slash spelling regression')
    def test_same_windows_file_with_different_slashes_rejects_without_normalization(self):
        other=copy.deepcopy(self.source)
        other['checkpoint']['path']=other['checkpoint']['path'].replace('\\','/')
        self.assertNotEqual(other,self.source)
        self.assertEqual(pin(other['checkpoint']['path'])['sha256'],self.source['checkpoint']['sha256'])
        self.evaluation(other)
        with self.assertRaisesRegex(ValueError,'non-injective model relocation'):
            prepare(self.spec,self.root/'package')
        self.assert_unpublished()

    def test_identical_original_references_across_training_and_bo3_remain_supported(self):
        self.evaluation(copy.deepcopy(self.source))
        result=prepare(self.spec,self.root/'package');manifest=read(result['manifest']['path'])
        self.assertEqual(len(manifest['model_source_relocations']),1)
        self.assertEqual(manifest['model_source_relocations'][0]['original'],self.source)


if __name__=='__main__':unittest.main(verbosity=2)
