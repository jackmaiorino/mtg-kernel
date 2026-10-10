"""Read-only canonical event wait, bounded two hours; never clears or acquires."""
import importlib.util
from pathlib import Path
import sys
import time
ROOT=Path(__file__).parent
WATCHER=ROOT.parent/'resume_qualifications.py'

def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);mod=importlib.util.module_from_spec(spec);sys.modules[name]=mod;spec.loader.exec_module(mod);return mod

def reservation_api():return load(ROOT/'baseline/python/tools/host_reservation_v1.py','clean_wait_reservations')

def wait_free(changed=None,deadline=None):
    reservations=reservation_api();watchers=load(WATCHER,'clean_directory_watch');watch=watchers.DirectoryChangeWait(reservations.lock_path().parent)
    began=time.monotonic(); deadline=deadline or began+7200;last=None
    try:
        status=reservations.status();refresh=time.monotonic()+600
        while True:
            compact={'canonical_state':status.get('state'),'generation':(status.get('record') or {}).get('generation'),'lane':(status.get('record') or {}).get('lane')}
            if compact!=last and changed:changed(compact)
            last=compact
            if status.get('state')=='free':return {'seconds':time.monotonic()-began,'final':compact,'scope':'Read-only canonical reservation queue; outside case timing'}
            if status.get('state')!='held':raise ValueError('unknown canonical reservation state')
            remaining=deadline-time.monotonic()
            if remaining<=0:raise TimeoutError('two-hour canonical reservation wait elapsed')
            signaled=watch.wait(min(60000,max(1,int(remaining*1000))))
            if signaled or time.monotonic()>=refresh:
                status=reservations.status();refresh=time.monotonic()+600
    finally:watch.close()
