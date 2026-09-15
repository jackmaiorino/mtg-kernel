"""Read-only regression against frozen real native artifacts, without execution.

The relocated trajectory is a derived in-memory fixture, not a cloud result.
The old wrapper is deliberately rejected as a timing reference, not repaired.
"""
import copy
import hashlib
from pathlib import Path
import tempfile
import unittest
from common import encoded, pin, read, write
from pack import Packager
from reference import build as build_reference
from semantic import trajectory_digest, validated_map
from throughput import complete_work_seconds
from test_cloud import timing_fixture

ROOT=Path('E:/mtg-kernel-learned-sideboarding-evidence/bo3-post480-preparation-001/phase1-training-qualification-001')
PINS={
    'serial.json':'e8c220abb2d21dd45fcddf7c97103e8508227a48c00b8cb8af2148c06820544d',
    'parallel10.json':'fc298a84b8bcd0bfbe0c1ae181d78e9bf49748d84ced9872354497b62460a5f9',
    'parallel10-launch-001/result.json':'0829c42600d29243c0c64026e8eb24c21dff14aed34c6d8ca11352d0b552a3c3',
    'serial-run/iterations/000000/attempt-000000/collect/episode-0000.json':'6becc530afbbd860e6f47a5454b4d560a499c27a4be14d43b948765e6201cc79',
    'parallel10-run/iterations/000000/attempt-000000/collect/episode-0000.json':'6becc530afbbd860e6f47a5454b4d560a499c27a4be14d43b948765e6201cc79',
}


@unittest.skipUnless(ROOT.exists(),'frozen native artifacts are unavailable on this machine')
class RealArtifactTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for name,digest in PINS.items():pin(ROOT/name,digest)
        cls.temporary=tempfile.TemporaryDirectory()
        cls.root=Path(cls.temporary.name)
        cls.original=read(ROOT/'serial.json')
        cls.parallel=read(ROOT/'parallel10.json')
        pack=Packager(cls.root/'package','/opt/phase1/real-artifact-relocation-regression')
        cls.relocated=copy.deepcopy(cls.original)
        cls.relocated['initial_source']=pack.model(cls.original['initial_source'])
        for opponent in cls.relocated['opponents']:
            opponent['source']=pack.model(opponent['source'])
        cls.manifest={'schema':'phase1-cloud-payload/v1','remote_root':str(pack.remote),
                      'model_source_relocations':pack.relocations,'files':pack.files}
        cls.manifest_path=cls.root/'package/payload/manifest.json'
        write(cls.manifest_path,cls.manifest)
        cls.rows=validated_map(pin(cls.manifest_path),cls.relocated)
        cls.first=read(ROOT/'serial-run/iterations/000000/attempt-000000/collect/episode-0000.json')

    @classmethod
    def tearDownClass(cls):cls.temporary.cleanup()

    def relocated_document(self,original):
        value=copy.deepcopy(original)
        def relocate(source):
            return copy.deepcopy(next(row['relocated'] for row in self.rows if row['original']==source))
        for seat in value['seat_behaviors']:seat['source']=relocate(seat['source'])
        value['episode']['opponent']=relocate(value['episode']['opponent'])
        return value

    def test_all_ten_real_serial_parallel_trajectories_and_exact_relocation(self):
        for index in range(10):
            name=f'iterations/000000/attempt-000000/collect/episode-{index:04d}.json'
            serial=read(ROOT/'serial-run'/name);parallel=read(ROOT/'parallel10-run'/name)
            with self.subTest(episode=index):
                canonical=trajectory_digest(serial,self.original)
                self.assertEqual(canonical,trajectory_digest(parallel,self.parallel))
                moved=self.relocated_document(parallel)
                self.assertNotEqual(hashlib.sha256(encoded(moved)).hexdigest(),
                                    hashlib.sha256(encoded(parallel)).hexdigest())
                self.assertEqual(canonical,trajectory_digest(moved,self.relocated,self.rows))

    def test_real_numeric_action_visible_and_identity_fields_are_never_stripped(self):
        original_digest=trajectory_digest(self.first,self.original)
        mutations=[
            lambda v:v['decisions'][0]['logits'].__setitem__(0,v['decisions'][0]['logits'][0]+.25),
            lambda v:v['decisions'][0].__setitem__('selected',v['decisions'][0]['selected']+1),
            lambda v:v['decisions'][0]['tensor']['state'].__setitem__(0,v['decisions'][0]['tensor']['state'][0]+.25),
            lambda v:v.__setitem__('behavior_state_sha256','0'*64),
            lambda v:v['seat_behaviors'][0]['identity'].__setitem__('state_sha256','0'*64),
            lambda v:v['source_import'].__setitem__('source_run_sha256','0'*64),
            lambda v:v['decisions'][0].__setitem__('unrelated_path','/this/is/not/a/model/reference'),
        ]
        for index,mutate in enumerate(mutations):
            value=self.relocated_document(self.first);mutate(value)
            with self.subTest(mutation=index):
                self.assertNotEqual(original_digest,trajectory_digest(value,self.relocated,self.rows))

    def test_real_unlisted_source_path_cannot_be_normalized(self):
        value=self.relocated_document(self.first)
        value['episode']['opponent']['play_import']['path']='/arbitrary/elsewhere.json'
        with self.assertRaisesRegex(ValueError,'outside pinned configuration'):
            trajectory_digest(value,self.relocated,self.rows)

    def test_actual_import_nonpath_change_is_not_an_allowed_relocation(self):
        manifest=copy.deepcopy(self.manifest);config=copy.deepcopy(self.relocated)
        row=manifest['model_source_relocations'][0]
        reference=row['relocated']['play_import']
        source=self.manifest_path.parent/Path(reference['path']).relative_to(manifest['remote_root'])
        changed=read(source);changed['unexpected_semantic_change']=True
        target=self.manifest_path.parent/'imports/changed-play-import.json';write(target,changed)
        replacement={'path':manifest['remote_root']+'/imports/changed-play-import.json','sha256':pin(target)['sha256']}
        manifest['files']['imports/changed-play-import.json']={'sha256':pin(target)['sha256'],'bytes':target.stat().st_size,'mode':420}
        for entry in manifest['model_source_relocations']:
            if entry['relocated']['play_import']==reference:entry['relocated']['play_import']=replacement
        config['initial_source']['play_import']=replacement
        path=self.manifest_path.with_name('changed-manifest.json');write(path,manifest)
        with self.assertRaisesRegex(ValueError,'beyond known path relocation'):
            validated_map(pin(path),config)

    def test_real_legacy_wrapper_is_rejected_not_recast_as_whole_timing(self):
        with self.assertRaisesRegex(ValueError,'fresh whole invocation'):
            build_reference(pin(ROOT/'parallel10.json'),pin(ROOT/'parallel10-launch-001/result.json'),
                            pin(ROOT/'build-cpu-001/manifest.json'))

    def test_real_native_duration_does_not_supply_missing_lifecycle_overhead(self):
        actual=read(ROOT/'parallel10-launch-001/result.json')
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            reference={'elapsed_seconds':actual['native_active_seconds'],
                       'runtime_evidence':pin(ROOT/'parallel10-launch-001/result.json')}
            write(root/'runtime.json',reference);rp=pin(root/'runtime.json')
            timing=timing_fixture(root,rp,'cloud',25)
            self.assertAlmostEqual(complete_work_seconds(timing,rp,reference,'cloud'),
                                   actual['native_active_seconds']+25)
            value=read(timing['path']);del value['phases']['input_transfer']
            write(root/'missing-overhead.json',value)
            with self.assertRaisesRegex(ValueError,'unavailable, not zero'):
                complete_work_seconds(pin(root/'missing-overhead.json'),rp,reference,'cloud')


if __name__=='__main__':unittest.main(verbosity=2)
