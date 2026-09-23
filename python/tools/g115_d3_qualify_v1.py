"""Bounded, portable whole-match throughput qualification, never formal D3.

The fixed consumed cohort is separate from the shared panel. Every worker
count completes the same whole matches and must reproduce serial store bytes.
RunPod execution requires the existing resident lease guard throughout.
"""
import argparse,concurrent.futures,copy,hashlib,json,os,pathlib,shutil,signal,subprocess,threading,time
P=pathlib.Path
def require(ok,message):
    if not ok:raise ValueError(message)
def read(path):return json.loads(P(path).read_bytes())
def sha(path):return hashlib.sha256(P(path).read_bytes()).hexdigest()
def checked(ref):
    path=P(ref['path']);require(sha(path)==ref['sha256'],'Input hash changed: '+str(path));return path
def write(path,value):
    path=P(path);temp=path.with_suffix(path.suffix+'.partial')
    with temp.open('w') as f:json.dump(value,f,indent=2,allow_nan=False);f.flush();os.fsync(f.fileno())
    os.replace(temp,path)
def sample(pid):
    try:
        if os.name=='nt':
            import psutil
            try:
                proc=psutil.Process(pid);cpu=proc.cpu_times();io=proc.io_counters()
                return dict(cpu=cpu.user+cpu.system,rss=proc.memory_info().rss,io=io.read_bytes+io.write_bytes)
            except psutil.Error:return None
        root=P('/proc')/str(pid);parts=(root/'stat').read_text().rsplit(')',1)[1].split()
        io={k:int(v) for k,v in (line.split(':') for line in (root/'io').read_text().splitlines())}
        return dict(cpu=(int(parts[11])+int(parts[12]))/os.sysconf('SC_CLK_TCK'),rss=int(parts[21])*os.sysconf('SC_PAGE_SIZE'),io=io['read_bytes']+io['write_bytes'])
    except (OSError,ProcessLookupError):return None
def free_memory():
    if os.name=='nt':
        import psutil
        return psutil.virtual_memory().available
    values={k:v.strip() for k,v in (line.split(':',1) for line in P('/proc/meminfo').read_text().splitlines())}
    available=int(values['MemAvailable'].split()[0])*1024
    for limit_path,current_path in (
        ('/sys/fs/cgroup/memory.max','/sys/fs/cgroup/memory.current'),
        ('/sys/fs/cgroup/memory/memory.limit_in_bytes','/sys/fs/cgroup/memory/memory.usage_in_bytes')):
        try:
            limit=P(limit_path).read_text().strip()
            if limit!='max':available=min(available,max(0,int(limit)-int(P(current_path).read_text())))
        except FileNotFoundError:pass
    return available

def validate(spec):
    require(spec['schema']=='g115-d3-throughput-qualification/v1','Wrong workload kind')
    require(spec['formal_measurement'] is False,'Qualification cannot launch formal measurement')
    require(spec['source_commit']=='e258daf3ab807cd6d8616a1431a21ea5ee22ac0b','Wrong native source')
    require(spec['host'] in ('jack','haleyspc','runpod'),'Unknown host')
    inventory=spec['inventory'];require(set(inventory)=={'jack','haleyspc','runpod'},'Three-host inventory required')
    require(inventory[spec['host']]['complete'] and 0<=time.time()-inventory['jack']['checked_unix']<=1800,'Fresh placement inventory required')
    counts=spec['worker_counts'];require(counts[0]==1 and counts==sorted(set(counts)) and len(counts)>=2 and max(counts)<=32,'Serial and increasing parallel comparison required')
    jobs=spec['jobs'];require(len(jobs)==64 and len({j['id'] for j in jobs})==64,'Expected fixed64-case engineering cohort')
    require(sum(j['arm']=='search' for j in jobs)==32 and sum(j['arm']=='baseline' for j in jobs)==32,'Both execution paths required')
    require(0<spec['job_timeout_seconds']<=1800 and 0<spec['group_timeout_seconds']<=7200,'Bounded qualification required')
    require(spec['reserve_bytes']>=(32 if spec['host']!='runpod' else 1)*2**30,'Memory reserve too small')
    forbidden=set(spec['formal_panel_seeds']);require(len(forbidden)==512,'Shared-panel exclusion missing')
    for job in jobs:
        command=job['command'];require(len(command['matches'])==1,'Whole BO3 jobs required')
        require(command['matches'][0]['config']['seed'] not in forbidden,'Qualification intersects formal panel')
        require(command['sources'][job['candidate_seat']]['source']['checkpoint']['sha256']=='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1','Wrong g115')
    for ref in spec['runtime_files']:checked(ref)
    require(any(str(checked(r))==spec['command_prefix'][-1] for r in spec['runtime_files']),'Native executable unpinned')

def qualify(spec,root):
    validate(spec);root=P(root);root.mkdir();write(root/'spec.json',spec)
    if os.environ.get('RUNPOD_POD_ID'):require(spec['host']=='runpod','Provider cannot bypass lease guard')
    guard=P(spec['guard_directory']) if spec['host']=='runpod' else None
    pod=os.environ.get('RUNPOD_POD_ID');lock=threading.Lock();active={};stop=threading.Event()
    started=time.monotonic();last_activity=last_productive=time.time();rows=[];phases=[];first_hashes=None
    def admission():
        require(free_memory()>=spec['reserve_bytes'],'Preserve memory reserve')
        require(shutil.disk_usage(root).free>=(1 if guard else 16)*2**30,'Preserve disk reserve')
        if guard:
            state=read(guard/'guard.json')
            require(pod and state['pod_id']==pod and state['name']==spec['lease_name'],'Lease identity differs')
            require(0<=time.time()-state['epoch']<=75 and state['provider_ok'] and state['allow_new_dispatch'] and not state['latched'],'Lease guard does not permit dispatch')
            require(not (guard/'stop-request.json').exists(),'Resident guard requested stop')
    def terminate(child):
        if child.poll() is not None:return
        if os.name=='nt':child.kill()
        else:os.killpg(child.pid,signal.SIGTERM)
        try:child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            if os.name=='nt':child.kill()
            else:os.killpg(child.pid,signal.SIGKILL)
            child.wait()
    def run(job,folder):
        nonlocal last_productive
        require(not stop.is_set(),'Qualification stopped');admission()
        folder.mkdir();command=copy.deepcopy(job['command']);command['output_directory']=str(folder/'outputs');write(folder/'request.json',command)
        env={k:v for k,v in os.environ.items() if os.name=='nt' and k.upper() in ('PATH','SYSTEMROOT','WINDIR','TEMP','TMP','COMSPEC')}
        env.update(PATH=env.get('PATH','/usr/local/bin:/usr/bin:/bin'),LANG='C.UTF-8',LC_ALL='C.UTF-8',CUDA_VISIBLE_DEVICES='',OMP_NUM_THREADS='1',MKL_NUM_THREADS='1',OPENBLAS_NUM_THREADS='1')
        kwargs=dict(creationflags=subprocess.CREATE_NO_WINDOW|subprocess.BELOW_NORMAL_PRIORITY_CLASS) if os.name=='nt' else dict(start_new_session=True)
        argv=spec['command_prefix']+[str(folder/'request.json')]
        if os.name!='nt':argv=['nice','-n','10']+argv
        begin=time.monotonic();row=dict(id=job['id'],arm=job['arm'],complete=False)
        with (folder/'native.log').open('wb') as log:
            child=subprocess.Popen(argv,env=env,stdout=log,stderr=subprocess.STDOUT,**kwargs)
            with lock:active[child.pid]=dict(child=child,cpu=0.,rss=0,io=0)
            try:
                while child.poll() is None:
                    require(not stop.is_set(),'Qualification interrupted')
                    require(time.monotonic()-begin<spec['job_timeout_seconds'],'Whole-match time bound reached')
                    time.sleep(.2)
                row['exit_code']=child.returncode;require(child.returncode==0,'Native match failed')
                completion=read(folder/'outputs/completion.json');output=folder/'outputs/match-000000.json'
                require(completion['matches']==1 and completion['match_sha256']==[sha(output)],'Native store incomplete')
                match=read(output);require(match['outcome'] in ({'winner':{'winner':0}},{'winner':{'winner':1}}),'Whole match did not terminate naturally')
                row.update(complete=True,sha256=sha(output),games=completion['natural_games'],decisions=completion['decisions'])
                last_productive=time.time()
            except Exception as e:row.update(error_type=type(e).__name__,error=str(e));stop.set()
            finally:
                terminate(child)
                with lock:stats=active.pop(child.pid)
                row.update(seconds=time.monotonic()-begin,cpu_seconds_sampled=stats['cpu'],peak_rss_sampled=stats['rss'],io_bytes_sampled=stats['io'])
                write(folder/'execution.json',row)
        return row
    result=dict(schema='g115-d3-throughput-result/v1',complete=False,host=spec['host'],phases=phases,formal_measurement=False)
    try:
        for workers in spec['worker_counts']:
            admission();folder=root/('workers-'+str(workers));folder.mkdir();begin=time.monotonic();phase=dict(workers=workers,rows=[]);phases.append(phase)
            with concurrent.futures.ThreadPoolExecutor(workers) as pool:
                futures=[pool.submit(run,j,folder/j['id']) for j in spec['jobs']]
                while not all(f.done() for f in futures):
                    try:admission();require(time.monotonic()-started<spec['group_timeout_seconds'],'Qualification time bound reached')
                    except Exception:stop.set();raise
                    with lock:
                        cpu=rss=io=0
                        for pid,stats in active.items():
                            now=sample(pid)
                            if now:
                                if now['cpu']>stats['cpu'] or now['io']>stats['io']:last_activity=time.time()
                                stats.update(cpu=now['cpu'],rss=max(stats['rss'],now['rss']),io=now['io'])
                            cpu+=stats['cpu'];rss+=stats['rss'];io+=stats['io']
                        progress=dict(epoch=time.time(),last_productive_epoch=last_productive,last_activity_epoch=last_activity,native_alive=bool(active),queued_work=bool(active) and any(not f.done() for f in futures),finished=False,workers=workers,active=len(active),cpu_seconds=cpu,rss_bytes=rss,io_bytes=io,completed=sum(f.done() for f in futures),pod_id=pod)
                    write(root/'progress.json',progress)
                    if guard:write(guard/'progress.json',progress)
                    time.sleep(1)
                phase['rows']=[f.result() for f in futures]
            require(all(r['complete'] for r in phase['rows']),'Incomplete cohort retained')
            phase.update(seconds=time.monotonic()-begin,matches=len(phase['rows']))
            phase['matches_per_second']=phase['matches']/phase['seconds']
            hashes={r['id']:r['sha256'] for r in phase['rows']}
            if first_hashes is None:first_hashes=hashes
            require(hashes==first_hashes,'Parallel match bytes differ from serial')
            phase['exact_serial_bytes']=True;write(folder/'completion.json',phase)
        result.update(complete=True,fastest_workers=max(phases,key=lambda p:p['matches_per_second'])['workers'])
    except Exception as e:result.update(error_type=type(e).__name__,error=str(e))
    finally:
        stop.set()
        with lock:
            for stats in active.values():terminate(stats['child'])
        # A queued future can reject after another job fails. Preserve the
        # completed job receipts even when gathering that future raises.
        for phase in phases:
            if not phase['rows']:
                folder=root/('workers-'+str(phase['workers']))
                phase['rows']=[read(folder/j['id']/'execution.json') for j in spec['jobs'] if (folder/j['id']/'execution.json').exists()]
                phase['not_started']=[j['id'] for j in spec['jobs'] if not (folder/j['id']/'execution.json').exists()]
        result['seconds']=time.monotonic()-started;write(root/'completion.json',result)
        if guard:write(guard/'progress.json',dict(pod_id=pod,epoch=time.time(),last_productive_epoch=last_productive,last_activity_epoch=time.time(),native_alive=False,queued_work=False,finished=True))
    return result

def main():
    p=argparse.ArgumentParser();p.add_argument('--spec',type=P,required=True);p.add_argument('--root',type=P,required=True);a=p.parse_args()
    result=qualify(read(a.spec),a.root);print(json.dumps({k:v for k,v in result.items() if k!='phases'}));raise SystemExit(0 if result['complete'] else 1)
if __name__=='__main__':main()
