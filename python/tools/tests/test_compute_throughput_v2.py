import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from compute_throughput_v2 import require_allocation
from public_training_dispatch_v2 import dispatch_qualified


class AllocationTests(unittest.TestCase):
    def put(self,name,value):
        path=self.root/name
        path.parent.mkdir(parents=True,exist_ok=True)
        path.write_text(json.dumps(value),encoding="utf-8")
        return dict(path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest())

    def load(self,item):
        return json.loads(Path(item["path"]).read_bytes())

    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        self.binary=self.put("native.exe","fixture executable")
        self.configs={job:self.put(f"{job}-config.json",dict(arm=job,gpu_ordinal=1,updates=[[{},{}]]*200)) for job in ["control","structured"]}
        self.jobs={job:dict(config_sha256=item["sha256"],updates=200) for job,item in self.configs.items()}
        devices={"jack":[(0,"GPU-J0"),(1,"GPU-J1")],"haleyspc":[(0,"GPU-H0")],"runpod":[]}
        inventory={host:dict(checked_at=datetime.now(timezone.utc).isoformat(),eligible=bool(items),reason="test inventory",
                   evidence=self.put(f"{host}-inventory.json",dict(host=host)),
                   devices=[dict(ordinal=o,uuid=u,eligible=True,reason="test device") for o,u in items]) for host,items in devices.items()}
        self.plan=dict(schema="public-training-allocation/v2",jobs=self.jobs,inventory=inventory,
                       candidates=[self.candidate("baseline",1,False,20),self.candidate("fast",4,True,8)],selected="fast")

    def candidate(self,label,workers,remote,elapsed):
        jobs={}
        for job in self.jobs:
            host="haleyspc" if remote and job=="structured" else "jack"
            device=1 if job=="control" else 0
            uuid="GPU-H0" if host=="haleyspc" else f"GPU-J{device}"
            placement=dict(host=host,gpu_ordinal=device,gpu_uuid=uuid,workers=workers)
            output=self.root/label/job/"outputs"
            origin=f"C:/remote/{label}/{job}/outputs" if host=="haleyspc" else str(output)
            request=self.put(f"{label}/{job}/request.json",dict(config=self.load(self.configs[job]),resume=None,stop_after=3,
                output_directory=origin,execution_gpu_ordinal=device,collector_workers=workers))
            execution=self.put(f"{label}/{job}/execution.json",dict(placement=placement,observed_gpu_uuid=uuid,
                binary_sha256=self.binary["sha256"],request_sha256=request["sha256"],exit_code=0,timeout=False,
                seconds=elapsed,started_unix=100,finished_unix=100+elapsed))
            completion=self.put(f"{label}/{job}/outputs/completion.json",dict(first_update=0,next_update=3,
                execution_gpu_ordinal=device,collector_workers=workers,receipts=[dict(update=i,episodes=2,natural_games=2,
                execution_gpu_ordinal=device,collector_workers=workers,seconds=elapsed/3) for i in range(3)]))
            outputs={f"{i:04}/{name}":self.put(f"{label}/{job}/outputs/{i:04}/{name}",dict(job=job,update=i,name=name))
                     for i in range(3) for name in ["checkpoint.json","optimizer.json","episode-000.json","episode-001.json"]}
            recovery=self.put(f"{label}/{job}/recovery.json",dict(source_output_directory=origin,files={name:p["sha256"] for name,p in outputs.items()}))
            jobs[job]=self.put(f"{label}/{job}/benchmark.json",dict(placement=placement,config=self.configs[job],binary=self.binary,
                execution=execution,request=request,completion=completion,outputs=outputs,recovery=recovery,
                source_output_directory=origin,local_output_directory=str(output)))
        return dict(id=label,benchmark=self.put(f"{label}/group.json",dict(mode="parallel",jobs=jobs,staging_seconds=0,recovery_seconds=0)),
                    setup_seconds=0,transfer_seconds=0,recovery_seconds=0)

    def mutate_report(self,candidate,job,field,update):
        item=self.plan["candidates"][candidate]
        group=self.load(item["benchmark"])
        report=self.load(group["jobs"][job])
        value=self.load(report[field]);update(value)
        relative=str(Path(report[field]["path"]).relative_to(self.root))
        report[field]=self.put(relative,value)
        group["jobs"][job]=self.put(str(Path(group["jobs"][job]["path"]).relative_to(self.root)),report)
        item["benchmark"]=self.put(str(Path(item["benchmark"]["path"]).relative_to(self.root)),group)

    def check(self):
        choice=self.put("choice.json",self.plan)
        return require_allocation(choice["path"],self.binary["sha256"],self.jobs)

    def test_selects_complete_remote_allocation(self):
        selected=self.check()
        self.assertEqual(selected["placements"]["structured"]["host"],"haleyspc")
        self.assertEqual(selected["placements"]["structured"]["gpu_ordinal"],0)

    def test_wrong_observed_gpu_rejected(self):
        self.mutate_report(1,"structured","execution",lambda d:d.update(observed_gpu_uuid="GPU-J0"))
        with self.assertRaisesRegex(ValueError,"execution device"):
            self.check()

    def test_remote_mapping_cannot_borrow_outputs(self):
        self.mutate_report(1,"structured","recovery",lambda d:d.update(source_output_directory="C:/another-run"))
        with self.assertRaisesRegex(ValueError,"recovery mapping"):
            self.check()

    def test_remote_corruption_rejected(self):
        self.mutate_report(1,"structured","recovery",lambda d:d["files"].update({"0000/optimizer.json":"wrong"}))
        with self.assertRaisesRegex(ValueError,"recovered bytes"):
            self.check()

    def test_parallel_shared_host_needs_actual_overlap(self):
        self.mutate_report(0,"structured","execution",lambda d:d.update(started_unix=200,finished_unix=220))
        with self.assertRaisesRegex(ValueError,"did not overlap"):
            self.check()

    def test_slower_allocation_rejected(self):
        self.plan["selected"]="baseline"
        with self.assertRaisesRegex(ValueError,"slower"):
            self.check()

    def test_unmeasured_eligible_device_rejected(self):
        self.plan["inventory"]["jack"]["devices"].append(dict(ordinal=2,uuid="GPU-J2",eligible=True,reason="available"))
        with self.assertRaisesRegex(ValueError,"lacks a measured"):
            self.check()

    def test_dispatch_uses_checked_devices(self):
        choice=self.put("choice.json",self.plan)
        with patch("public_training_dispatch_v2._dispatch_group",return_value="dispatched") as launch:
            self.assertEqual(dispatch_qualified(self.root/"run",self.binary,self.configs,choice["path"],1800),"dispatched")
            placements=launch.call_args.args[3]
            self.assertEqual(placements["structured"],dict(host="haleyspc",gpu_ordinal=0,gpu_uuid="GPU-H0",workers=4))

    def test_invalid_choice_never_dispatches(self):
        self.plan["selected"]="baseline"
        choice=self.put("choice.json",self.plan)
        with patch("public_training_dispatch_v2._dispatch_group") as launch:
            with self.assertRaises(ValueError):
                dispatch_qualified(self.root/"run",self.binary,self.configs,choice["path"],1800)
            launch.assert_not_called()


if __name__=="__main__":
    unittest.main()
