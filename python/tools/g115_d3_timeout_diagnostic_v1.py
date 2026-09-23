"""Guarded diagnosis of one retained timeout; never admits formal measurement."""
import argparse, concurrent.futures, copy, hashlib, json, os, pathlib, subprocess, sys, threading, time
import psutil
from g115_active_compute_v1 import controllers
P=pathlib.Path
SOURCE='fe409f455276c0025372d7a5abf92df4191d7293'
BINARY='4c575904b197e54701ccda9399b6223ef000890dab1f3eb78068340ca37d118f'
PANEL='eaca43dc397894d7a8726401ca32d3240e862a0ae82274900330d9b5ab4a9dcc'
CONTROLS=['c12-r3-s0-search','c18-r7-s1-search']
TARGET='c18-r3-s1-search'
SEARCH='5eb1d55d13b78b341f8ff0c4df2589f8ee725974dc9fcadd691db6e12b2133d7'
def read(p):return json.loads(P(p).read_bytes())
def sha(p):return hashlib.sha256(P(p).read_bytes()).hexdigest()
def pin(p):return dict(path=str(p),sha256=sha(p))
def checked(ref):
    if sha(ref['path'])!=ref['sha256']:raise ValueError('Changed pinned file: '+ref['path'])
    return P(ref['path'])
def require(ok,message):
    if not ok:raise ValueError(message)
def write(p,value):
    with P(p).open('x') as f:json.dump(value,f,indent=2,allow_nan=False)
def validate(spec,inventory):
    require(spec['schema']=='g115-timeout-diagnostic/v1' and spec['formal_measurement'] is False,'Diagnostic scope only')
    require(spec['source']==SOURCE and spec['binary']['sha256']==BINARY,'Unverified diagnostic executable')
    checked(spec['binary']);require(spec['panel']['sha256']==PANEL,'Changed shared panel');panel=read(checked(spec['panel']))
    for ref in spec['readers']:checked(ref)
    require(spec['host'] in ('jack','haleyspc'),'Only verified Windows diagnostic runtime supported')
    require(set(inventory)=={'jack','haleyspc','runpod'} and inventory['runpod']['complete'] and inventory['runpod']['http_status']==200,'Corrected-UA three-host inventory required')
    for host in ('jack','haleyspc'):
        require(0<=time.time()-inventory[host]['checked_unix']<=600,'Stale fleet inventory')
        if inventory[host]['complete']:require(isinstance(inventory[host]['data'].get('competing_controllers'),list),'Coordinator census missing')
    require(inventory[spec['host']]['complete'] and not inventory[spec['host']]['data']['competing_native'],'Selected host occupied')
    require(inventory[spec['host']]['data'].get('competing_controllers')==[],'Live coordinator window occupied or census missing')
    accepted=read(checked(spec['acceptance']))
    require(accepted['complete'] and accepted['source']==SOURCE and accepted['native_stores_verified']==4 and accepted['exact_logging_pairs']==2 and accepted['progress_events_verified']==478 and accepted['frozen_formal_semantic_equal'],'Logging acceptance missing')
    jobs={j['id']:j for j in panel['jobs']}
    require(set(spec['commands'])==set(CONTROLS+[TARGET]),'Changed fixed diagnostic scope')
    for identifier,ref in spec['commands'].items():
        command=read(checked(ref));expected=jobs[identifier];seat=expected['candidate_seat']
        require(command['matches']==expected['base_command']['matches'] and len(command['matches'])==1,'Changed seed, decks or native limits')
        require(command['sources'][seat]['kind']=='information_set_search_v3','Search route missing')
        descriptor=command['sources'][seat]['descriptor']
        require(hashlib.sha256(json.dumps(descriptor,sort_keys=True,separators=(',',':')).encode()).hexdigest()==SEARCH,'Changed search budget or model')
        require(command['sources'][seat]['source']['checkpoint']['sha256']=='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1','Wrong incumbent')
    return jobs
def owners(reserve,host):
    require(psutil.virtual_memory().available>=reserve,'Memory reserve unavailable')
    require(not any(host in row['hosts'] for row in controllers()),'Active multirun coordinator window')
    for proc in psutil.process_iter(['name']):
        name=(proc.info['name'] or '').lower()
        require(not(name.startswith('mtg_kernel') or name in ('public_feature_evaluation_v1.exe','cargo.exe','rustc.exe','trainer.exe')),'Competing native/build work')
def last_progress(folder):
    path=folder/'native.log'
    if not path.exists():return None
    with path.open('rb') as f:
        f.seek(max(0,path.stat().st_size-16384));lines=f.read().decode('utf8',errors='replace').splitlines()
    for line in reversed(lines):
        try:row=json.loads(line)
        except ValueError:continue
        if isinstance(row,dict) and row.get('schema')=='g115-d3-decision-progress/v1':return row
    return None
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--spec',type=P,required=True);parser.add_argument('--inventory',type=P,required=True);parser.add_argument('--root',type=P,required=True);parser.add_argument('--choice',type=P);args=parser.parse_args()
    spec=read(args.spec);inventory=read(args.inventory);jobs=validate(spec,inventory)
    sys.path.insert(0,spec['reader_directory'])
    from g115_d3_native_results_v1 import read_match
    from g115_d3_baseline_parity_v1 import normalized_pair
    reserve=(32 if spec['host']=='jack' else 8)*2**30
    owners(reserve,spec['host']);require(psutil.disk_usage(args.root.anchor).free>=16*2**30,'Disk reserve unavailable')
    if args.choice:
        choice=read(args.choice);require(choice['schema']=='g115-single-timeout-placement/v1' and choice['target']==TARGET and choice['workers']==1,'One sequential timeout case only')
        require(choice['host']==spec['host'] and choice['inventory']==pin(args.inventory),'Placement inventory mismatch')
        eligible=[h for h in ('jack','haleyspc') if inventory[h]['complete'] and not inventory[h]['data']['competing_native'] and inventory[h]['data']['competing_controllers']==[]]
        require(set(choice['qualifications'])==set(eligible),'Every currently clear Windows host must be qualified')
        candidates={}
        for host,ref in choice['qualifications'].items():
            q=read(checked(ref));require(q['complete'] and q['host']==host and q['source']==SOURCE and q['binary_sha256']==BINARY and q['controls']==CONTROLS and q['serial_parallel_exact'],'Incompatible qualification')
            require([p['workers'] for p in q['phases']]==[1,2],'Serial/parallel measurements missing')
            candidates[host]=q['phases'][0]['seconds']+choice['remaining_transfer_seconds'][host]
        require(spec['host']==min(candidates,key=candidates.get),'Not fastest qualified single-match placement')
        require(choice['runpod_disposition']=='No qualified compatible logging runtime; retained uninstrumented cloud serial benchmark is slower than Jack; no paid acceleration established','Cloud disposition required')
    args.root.mkdir();write(args.root/'manifest.json',dict(spec=pin(args.spec),inventory=pin(args.inventory),choice=pin(args.choice) if args.choice else None,launcher=pin(__file__),controller_census=pin(P(__file__).with_name('g115_active_compute_v1.py')),formal_measurement=False,source=SOURCE,workers=1 if args.choice else [1,2],per_job_bound_seconds=1800 if args.choice else 300,reserve_bytes=reserve,gpu_ordinal=None,scope='One retained timeout, unchanged bound; or fixed consumed correctness/timing controls. No outcomes aggregated.'))
    descriptor=read(checked(spec['v3_source']));envelope=read(checked(descriptor['transfer_envelope']))
    envelope['receipt']['destination_build_git_head']=SOURCE;write(args.root/'envelope.json',envelope)
    descriptor['transfer_envelope']=pin(args.root/'envelope.json');write(args.root/'v3-source.json',descriptor)
    stop=threading.Event()
    def execute(identifier,folder,bound):
        folder.mkdir();row=dict(id=identifier,complete=False);child=None;start=time.monotonic();cpu=0.;peak=0;next_report=start+60
        try:
            require(not stop.is_set(),'Peer execution failed')
            command=read(checked(spec['commands'][identifier]));seat=jobs[identifier]['candidate_seat'];command['sources'][1-seat]['source']['play_import']=pin(args.root/'v3-source.json');command['output_directory']=str(folder/'outputs');write(folder/'request.json',command)
            env=os.environ.copy();env.update(CUDA_VISIBLE_DEVICES='',OMP_NUM_THREADS='1',MKL_NUM_THREADS='1',OPENBLAS_NUM_THREADS='1',MTG_D3_PROGRESS_DIAGNOSTIC='1')
            with (folder/'native.log').open('xb') as log:
                child=subprocess.Popen([str(checked(spec['binary'])),str(folder/'request.json')],env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW|subprocess.BELOW_NORMAL_PRIORITY_CLASS)
                write(folder/'process.json',dict(pid=child.pid,epoch=time.time()))
                while child.poll() is None:
                    require(not stop.is_set(),'Peer execution failed')
                    require(psutil.virtual_memory().available>=reserve,'Memory reserve reached')
                    require(psutil.disk_usage(args.root.anchor).free>=16*2**30,'Disk reserve reached')
                    if time.monotonic()-start>=bound:raise TimeoutError('Unchanged diagnostic time bound reached')
                    try:
                        proc=psutil.Process(child.pid);cpu=sum(proc.cpu_times()[:2]);peak=max(peak,proc.memory_info().rss)
                    except psutil.NoSuchProcess:pass
                    if time.monotonic()>=next_report:
                        print(json.dumps(dict(id=identifier,elapsed_seconds=time.monotonic()-start,cpu_seconds=cpu,peak_rss_bytes=peak,last_progress=last_progress(folder))),flush=True);next_report+=60
                    time.sleep(.1)
            require(child.returncode==0,'Native diagnostic failed')
            actual=dict(request=pin(folder/'request.json'),output_directory=command['output_directory'])
            validated=read_match(jobs[identifier],actual,SOURCE,spec['models'])
            if identifier in CONTROLS:
                old,new=normalized_pair(read(checked(spec['references'][identifier])),read(folder/'outputs/match-000000.json'),seat,[descriptor['transfer_envelope']['sha256']]);require(old==new,'Control differs from retained formal store')
            row.update(complete=True,sha256=validated['sha256'],decisions=validated['decisions'],games=validated['games'])
        except Exception as error:
            stop.set();row.update(error_type=type(error).__name__,error=str(error))
        finally:
            if child is not None and child.poll() is None:child.kill();child.wait()
            row.update(seconds=time.monotonic()-start,cpu_seconds_sampled=cpu,peak_rss_bytes_sampled=peak,last_progress=last_progress(folder));write(folder/'receipt.json',row)
        return row
    phases=[];phase_error=None
    for workers in ([1] if args.choice else [1,2]):
        try:owners(reserve,spec['host'])
        except (ValueError,OSError) as error:
            phase_error=dict(error_type=type(error).__name__,error=str(error),before_workers=workers);break
        folder=args.root/('workers-'+str(workers));folder.mkdir();start=time.monotonic()
        selected=[TARGET] if args.choice else CONTROLS
        with concurrent.futures.ThreadPoolExecutor(workers) as pool:rows=list(pool.map(lambda identifier:execute(identifier,folder/identifier,1800 if args.choice else 300),selected))
        phase=dict(workers=workers,seconds=time.monotonic()-start,rows=rows);write(folder/'completion.json',phase);phases.append(phase)
        if not all(row['complete'] for row in rows):break
    complete=phase_error is None and len(phases)==(1 if args.choice else 2) and all(row['complete'] for phase in phases for row in phase['rows'])
    exact=None if args.choice else len(phases)==2 and [r.get('sha256') for r in phases[0]['rows']]==[r.get('sha256') for r in phases[1]['rows']]
    result=dict(schema='g115-timeout-diagnostic-result/v1',complete=complete and (args.choice is not None or exact),host=spec['host'],source=SOURCE,binary_sha256=BINARY,controls=CONTROLS,serial_parallel_exact=exact,phases=phases,phase_error=phase_error,formal_measurement=False,formal_verdict=None,strength_statistics=None)
    write(args.root/'completion.json',result);print(json.dumps({k:v for k,v in result.items() if k!='phases'}),flush=True)
    return 0 if result['complete'] else 1
if __name__=='__main__':raise SystemExit(main())
