"""WMI audit owner with spawn-time child QoS and read-only progress observation."""
import argparse
import ctypes
from ctypes import wintypes as W
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time


def require(ok,message):
    if not ok:raise ValueError(message)


def checked(ref):
    with Path(ref['path']).open('rb') as f:actual=hashlib.file_digest(f,'sha256').hexdigest()
    require(actual==ref['sha256'],'Pinned observer input changed')
    return Path(ref['path'])


def cpu_ticks():
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    idle,kernel_time,user=W.FILETIME(),W.FILETIME(),W.FILETIME()
    require(kernel.GetSystemTimes(ctypes.byref(idle),ctypes.byref(kernel_time),ctypes.byref(user)),'CPU accounting unavailable')
    value=lambda v:(v.dwHighDateTime<<32)|v.dwLowDateTime
    return value(idle),value(kernel_time)+value(user)


def execute(args):
    plan=json.loads(Path(args.manifest).read_bytes())
    require(checked(plan['observer']).resolve()==Path(__file__).resolve(),'Wrong observer')
    launcher=checked(plan['documents']['launcher'])
    require(launcher.name=='g115_d4_audit_pipeline_v1.py' and plan['mode'] in ('qualification','production'),'Only the guarded audit is observed')
    policies=[r for r in plan['dependencies'] if Path(r['path']).name=='windows_owned_child_policy_v1.py']
    require(len(policies)==1,'Exactly one pinned owned-child policy required')
    spec=importlib.util.spec_from_file_location('audit_owned_child_policy',checked(policies[0]))
    policy=importlib.util.module_from_spec(spec);spec.loader.exec_module(policy)
    root=Path(args.root);report=Path(args.report_root)
    require(root.resolve()==Path(plan['worker_root']).resolve() and not root.exists() and not report.exists(),'Fresh observed run required')
    report.mkdir();work=json.loads(checked(plan['workload']).read_bytes());workers=plan['workers'][0]
    evidence=json.loads(checked(plan['throughput']).read_bytes()) if 'throughput' in plan else {'phases':[]}
    total_inputs=len(work['groups'])*len(plan['workers']);total_updates=len(work['commands'])*len(plan['workers'])
    journal=report/'utilization.jsonl';started=time.monotonic();previous=cpu_ticks();offset=0;sealed_inputs=sealed_scores=0;low=0
    completed={'complete':False,'read_only_observation':True,'owned_child_scheduling_at_spawn':True,'pid':None}
    proc=None
    try:
        with (report/'pipeline.stdout').open('xb') as out,(report/'pipeline.stderr').open('xb') as err:
            proc=subprocess.Popen([os.sys.executable,'-B','-u',str(launcher),'--manifest',args.manifest,'--host',args.host,'--root',args.root],stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW|subprocess.BELOW_NORMAL_PRIORITY_CLASS)
            completed['pid']=proc.pid
            completed['scheduling']=policy.configure_owned_child(proc,os.sys.executable)
            while True:
                window_start=time.monotonic()
                try:proc.wait(timeout=60)
                except subprocess.TimeoutExpired:pass
                now=cpu_ticks();total=now[1]-previous[1];busy=1-(now[0]-previous[0])/total if total>0 else None;previous=now
                storage=root/'control/storage.jsonl'
                if storage.exists():
                    with storage.open('rb') as f:
                        f.seek(offset)
                        for raw in f:
                            if not raw.endswith(b'\n'):break
                            offset+=len(raw);row=json.loads(raw)
                            if row['event']=='sealed' and not row['detail']['failed']:
                                name=row['detail']['name']
                                workers=int(name.split('-',1)[0][1:])
                                sealed_inputs+=('-input-' in name);sealed_scores+=('-score-' in name)
                pending=sealed_scores<total_updates
                phase='scoring_and_all_record_signal' if sealed_inputs//len(work['groups'])>sealed_scores//len(work['commands']) else 'archive_staging'
                duration=time.monotonic()-window_start
                low=low+1 if duration>=59 and busy is not None and busy<0.90 and pending else 0
                row={'elapsed_seconds':time.monotonic()-started,'window_seconds':duration,'system_cpu_busy_fraction':busy,'cpu_scope':'All host logical CPUs; includes unrelated activity, not owned-child CPU attribution.','phase':phase,'sealed_input_groups':sealed_inputs,'total_input_groups':total_inputs,'completed_updates':sealed_scores,'total_updates':total_updates,'workers':workers,'under90_consecutive_windows':low,'process_exited':proc.poll() is not None}
                if low>=2:
                    row['diagnosis']={'trigger':'Two consecutive60s windows under90% while assigned work remains','active_phase':phase,'measured_worker_comparison':evidence['phases'],'action':'Retain the fastest measured worker count and record capacity shortfall; no artificial load or unqualified concurrency change.','followup_required':True,'qualification_limit':'Matrix tested1/4/8 workers; no claim that unused cores were fully qualified or continuously CPU-ready.'}
                require(not journal.exists() or journal.stat().st_size<8*1024**2,'Observer journal bound reached')
                with journal.open('a',encoding='utf-8') as f:f.write(json.dumps(row,separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
                if proc.poll() is not None:break
            completed.update(complete=proc.returncode==0,exit_code=proc.returncode,seconds=time.monotonic()-started)
    except BaseException as error:
        completed['error']=repr(error)
        # Observation failure does not kill a healthy qualified owner. Wait for its
        # own guarded completion; the outer receipt remains failed for follow-up.
        if proc is not None and proc.poll() is None:proc.wait()
        raise
    finally:
        with (report/'completion.json').open('x') as f:json.dump(completed,f,indent=2)
    return proc.returncode


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['manifest','host','root','report-root']:parser.add_argument('--'+name,required=True)
    raise SystemExit(execute(parser.parse_args()))
