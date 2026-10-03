import json,pathlib,sys,tempfile,unittest
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'tools'))
import g115_active_compute_v1 as census

class Process:
    info={'name':'python.exe'}
    pid=7
    def __init__(self,argv,cwd):self.argv=argv;self.directory=cwd
    def cmdline(self):return self.argv
    def cwd(self):return self.directory
    def create_time(self):return 42.

class ActiveComputeTests(unittest.TestCase):
    def collect(self,argv,cwd='.'):
        with patch.object(census.psutil,'process_iter',return_value=[Process(argv,cwd)]):return census.controllers()
    def test_qualification_retains_cross_host_window_between_batches(self):
        rows=self.collect(['python','D:/tools/multirun_launcher_v1.py','qualify','--allocation','1@0','--allocation','2@0+2@1+3@haleyspc:0'])
        self.assertEqual(rows[0]['hosts'],['haleyspc','jack'])
    def test_local_only_launch_does_not_reserve_haley(self):
        with tempfile.TemporaryDirectory() as folder:
            pathlib.Path(folder,'choice.json').write_text(json.dumps(dict(selected='local',candidates=[dict(id='local',hosts=['jack']),dict(id='both',hosts=['jack','haleyspc'])])))
            rows=self.collect(['python','D:/tools/multirun_launcher_v1.py','launch','--choice','choice.json'],folder)
            self.assertEqual(rows[0]['hosts'],['jack'])
            self.assertIn('choice_sha256',rows[0])
    def test_unresolved_live_placement_is_not_treated_as_idle(self):
        with tempfile.TemporaryDirectory() as folder:
            rows=self.collect(['python','D:/tools/multirun_launcher_v1.py','launch','--choice','missing.json'],folder)
            self.assertEqual(rows[0]['hosts'],['jack','haleyspc'])
            self.assertIn('placement_error',rows[0])
    def test_readonly_or_other_python_work_does_not_reserve_fleet(self):
        for argv in (['python','D:/tools/multirun_launcher_v1.py','inspect'],['python','D:/tools/unrelated.py','launch']):
            self.assertEqual(self.collect(argv),[])

if __name__=='__main__':unittest.main()
