"""R8/V2 baseline-only engineering precheck. No strength analysis or search."""
import argparse, concurrent.futures, copy, json, os, pathlib, subprocess, threading, time
import psutil
from g115_d3_timeout_diagnostic_v1 import checked, pin, read, require, write, owners
from g115_d3_native_results_v1 import read_match
from g115_d3_baseline_parity_v1 import normalized_pair

P=pathlib.Path
SOURCE='cd41885e0ac05586d89bd4b2b7fb1284248689ef'
REVERSE='e990740a76726bb05f34bd2dee872f89e55f0671'
REVERSE_BINARY='f1069ab05911dc05e5a1a4ccd3ed7a55448bf250feee9bd79326e51307782517'
PANEL='eaca43dc397894d7a8726401ca32d3240e862a0ae82274900330d9b5ab4a9dcc'
CONTROLS=['c00-r0-s0-baseline','c10-r3-s0-baseline','c18-r0-s0-baseline','c19-r0-s1-baseline',
          'c28-r0-s0-baseline','c36-r0-s0-baseline','c45-r0-s0-baseline','c63-r0-s0-baseline']

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--spec',type=P,required=True);ap.add_argument('--inventory',type=P,required=True);ap.add_argument('--root',type=P,required=True);a=ap.parse_args()
    spec=read(a.spec);inv=read(a.inventory);host=spec['host']
    require(spec['schema']=='g115-d3-parity-precheck-spec/v1' and spec['formal_measurement'] is False,'Baseline engineering only')
    require(host in ('desktop','computehost') and inv[host]['complete'],'Unavailable Windows host')
    require(set(inv)=={'desktop','computehost','runpod'} and inv['runpod']['complete'] and inv['runpod']['http_status']==200,'Corrected-UA fleet inventory required')
    require(all(0<=time.time()-inv[h]['checked_unix']<=600 for h in inv),'Stale inventory')
    require(not inv[host]['data']['competing_native'] and inv[host]['data']['competing_controllers']==[],'Occupied host')
    other='desktop' if host=='computehost' else 'computehost'
    require(not inv[other]['complete'] or bool(inv[other]['data']['competing_native'] or inv[other]['data']['competing_controllers']), 'Other Windows host is clear: compare its qualification before selecting this finite single-host pass')
    require(spec['panel']['sha256']==PANEL,'Changed shared panel')
    panel=read(checked(spec['panel']));expected={j['id']:j for j in panel['jobs'] if j['arm']=='baseline'}
    require(len(expected)==1024 and set(spec['commands'])==set(spec['references'])==set(expected),'All 1024 baseline conditions required')
    build=read(checked(spec['formal_build']))
    require(build['complete'] and build['binaries']['public_feature_evaluation_v1']['sha256']==spec['binary']['sha256'],'Formal executable differs from verified build')
    require(spec['reverse_binary']['sha256']==REVERSE_BINARY,'Wrong reversal executable')
    checked(spec['binary']);checked(spec['reverse_binary']);checked(spec['toolchain'])
    for name,ref in spec['code'].items():require(pin(P(__file__).with_name(name))['sha256']==ref['sha256'],'Changed precheck code')
    for identifier,ref in spec['commands'].items():
        command=read(checked(ref));job=expected[identifier];seat=job['candidate_seat']
        require(command['matches']==job['base_command']['matches'] and command['sources'][seat]['kind']=='legacy','Changed baseline condition')
        require(command['sources'][seat]['source']['checkpoint']['sha256']=='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1','Wrong g115')
        require(command['sources'][1-seat]['kind']=='legacy' and command['sources'][1-seat]['v3_forced_actions'] and command['sources'][1-seat]['v3_spell_target_reference_adapter'],'Wrong opponent route')
        checked(spec['references'][identifier])
    reserve=(32 if host=='desktop' else 8)*2**30
    owners(reserve,host);require(psutil.disk_usage(a.root.anchor).free>=16*2**30,'Disk reserve unavailable')
    a.root.mkdir();write(a.root/'manifest.json',dict(spec=pin(a.spec),inventory=pin(a.inventory),source=SOURCE,reverse_source=REVERSE,workers=[1,4,8],controls=CONTROLS,per_job_timeout_seconds=60,whole_precheck_timeout_seconds=3600,launcher=pin(__file__),formal_measurement=False,gpu_ordinal=None))
    original=read(checked(spec['v3_source']));env=read(checked(original['transfer_envelope']))
    require(env['receipt']['destination_build_git_head']==SOURCE,'Formal provenance differs')
    formal_envelope=original['transfer_envelope']['sha256']
    env['receipt']['destination_build_git_head']=REVERSE;write(a.root/'reverse-envelope.json',env)
    rev=copy.deepcopy(original);rev['transfer_envelope']=pin(a.root/'reverse-envelope.json');write(a.root/'reverse-source.json',rev)
    stop=threading.Event();start=time.monotonic()
    def execute(identifier,root,reversal=False):
        folder=root/identifier;folder.mkdir();row=dict(id=identifier,complete=False);child=None;begin=time.monotonic()
        try:
            require(not stop.is_set(),'Peer failure')
            command=read(checked(spec['commands'][identifier]));seat=expected[identifier]['candidate_seat']
            if reversal:command['sources'][1-seat]['source']['play_import']=pin(a.root/'reverse-source.json')
            command['output_directory']=str(folder/'outputs');write(folder/'request.json',command)
            runtime=os.environ.copy();runtime.update(CUDA_VISIBLE_DEVICES='',OMP_NUM_THREADS='1',MKL_NUM_THREADS='1',OPENBLAS_NUM_THREADS='1')
            with (folder/'native.log').open('xb') as log:
                child=subprocess.Popen([str(checked(spec['reverse_binary'] if reversal else spec['binary'])),str(folder/'request.json')],env=runtime,stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW|subprocess.BELOW_NORMAL_PRIORITY_CLASS)
                write(folder/'process.json',dict(pid=child.pid,epoch=time.time()))
                while child.poll() is None:
                    require(not stop.is_set(),'Peer failure');require(time.monotonic()-begin<60 and time.monotonic()-start<3600,'Diagnostic bound reached')
                    require(psutil.virtual_memory().available>=reserve and psutil.disk_usage(a.root.anchor).free>=16*2**30,'Resource reserve reached');time.sleep(.1)
            require(child.returncode==0,'Native precheck failed')
            result=read_match(expected[identifier],dict(request=pin(folder/'request.json'),output_directory=command['output_directory']),REVERSE if reversal else SOURCE,spec['models'])
            store=folder/'outputs/match-000000.json'
            old,new=normalized_pair(read(checked(spec['references'][identifier])),read(store),seat,[rev['transfer_envelope']['sha256'] if reversal else formal_envelope])
            differences=sorted(k for k in set(old)|set(new) if old.get(k)!=new.get(k) or (k in old)!=(k in new))
            row.update(complete=True,store=pin(store),decisions=result['decisions'],semantic_equal=not differences,differing_fields=differences,raw_equal=pin(store)['sha256']==spec['references'][identifier]['sha256'])
        except Exception as error:stop.set();row.update(error=str(error))
        finally:
            if child is not None and child.poll() is None:child.kill();child.wait()
            row['seconds']=time.monotonic()-begin;write(folder/'receipt.json',row)
        return row
    def phase(name,ids,workers,reversal=False):
        owners(reserve,host);folder=a.root/name;folder.mkdir();begin=time.monotonic()
        with concurrent.futures.ThreadPoolExecutor(workers) as pool:rows=list(pool.map(lambda i:execute(i,folder,reversal),ids))
        result=dict(workers=workers,seconds=time.monotonic()-begin,rows=rows);write(folder/'completion.json',result)
        require(all(r['complete'] for r in rows),'Incomplete precheck phase retained')
        return result
    result=dict(schema='g115-d3-parity-precheck/v1',complete=False,formal_measurement=False,formal_verdict=None)
    try:
        phases=[phase('workers-'+str(w),CONTROLS,w) for w in (1,4,8)]
        require(all([r['store']['sha256'] for r in p['rows']]==[r['store']['sha256'] for r in phases[0]['rows']] for p in phases),'Parallel output differs')
        reverse_phases=[phase('reverse-workers-'+str(w),CONTROLS,w,True) for w in (1,4,8)]
        require(all([r['store']['sha256'] for r in p['rows']]==[r['store']['sha256'] for r in reverse_phases[0]['rows']] for p in reverse_phases),'Reversal parallel output differs')
        require(all(r['semantic_equal'] for p in reverse_phases for r in p['rows']),'Reversal control differs from archive')
        index=min(range(3),key=lambda i:phases[i]['seconds']+reverse_phases[i]['seconds']);fastest=phases[index];workers=fastest['workers']
        require(len(expected)*(fastest['seconds']+reverse_phases[index]['seconds'])/len(CONTROLS)+time.monotonic()-start<3600,'Projected diagnostic exceeds bound')
        full=phase('current',sorted(expected),workers)
        mismatches=[r['id'] for r in full['rows'] if not r['semantic_equal']]
        reversal=phase('reversal',mismatches,workers,True) if mismatches else dict(rows=[],seconds=0)
        require(all(r['semantic_equal'] for r in reversal['rows']),'Unexplained baseline drift')
        result.update(complete=True,source=SOURCE,reverse_source=REVERSE,binary_sha256=spec['binary']['sha256'],reverse_binary_sha256=REVERSE_BINARY,host=host,phases=phases,reverse_phases=reverse_phases,workers=workers,current=full,reversal=reversal,raw_mismatches=sum(not r['raw_equal'] for r in full['rows']),semantic_mismatches=len(mismatches),v3_envelopes=[formal_envelope],reverse_envelopes=[rev['transfer_envelope']['sha256']],spec=pin(a.spec))
    except Exception as error:result['error']=str(error)
    result['seconds']=time.monotonic()-start;write(a.root/'completion.json',result);print(json.dumps({k:v for k,v in result.items() if k not in ('phases','reverse_phases','current','reversal')}),flush=True)
    return 0 if result['complete'] else 1
if __name__=='__main__':raise SystemExit(main())
