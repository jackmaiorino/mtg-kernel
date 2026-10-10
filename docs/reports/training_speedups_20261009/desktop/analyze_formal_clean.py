"""Read-only full ABBA analysis. Requires terminal successful resume; never polls."""
import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT=Path('D:/training-speedups-20261009/desktop')
HELPER=ROOT.parent/'desktop_helpers/summarize_cases.py'

def require(ok,msg):
    if not ok: raise ValueError(msg)

def pin(p):
    p=Path(p)
    with p.open('rb') as f: digest=hashlib.file_digest(f,'sha256').hexdigest()
    return dict(path=str(p),sha256=digest)

def read(p): return json.loads(Path(p).read_bytes())
def checked(ref):
    require(pin(ref['path'])==ref,'pin changed: '+ref['path'])
    return read(ref['path'])

def write(p,value):
    with p.open('x',encoding='utf-8') as f: json.dump(value,f,indent=2,allow_nan=False);f.write('\n')

def capture(value):return str(value).replace('\\','/').casefold()

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--state',type=Path,default=ROOT/'formal-v3/coordinator/state.json')
    ap.add_argument('--out',type=Path,default=ROOT/'formal-v3/analysis')
    args=ap.parse_args()
    statepin=pin(args.state);state=checked(statepin)
    require(state.get('complete') is True and not state.get('error'),'full coordinator not successfully complete')
    finalcold=Path('E:/training-speedups-20261009/formal-v3-final-state.json')
    require(pin(finalcold)['sha256']==statepin['sha256'],'terminal state lacks exact verified E copy')
    labels=['clean-01-baseline','clean-02-candidate','clean-03-candidate','clean-04-baseline']
    require([x['label'] for x in state['completed']]==labels,'all four ordered completed cases required')
    require(state['sequential_nonoverlapping'] is True,'nonoverlap declaration absent')
    plan=checked(state['plan']);prior=state;cases=[];telemetry=[];fingerprints=[]
    metadata=[]
    for row,command in zip(state['completed'],plan['commands']):
        label=row['label'];report=checked(row['report']);fingerprints.append(report['fingerprint'])
        require(report['completed_updates']==162 and report['completed_games']==1620 and report['complete'],'full counts incomplete')
        inspect=checked(row['inspect']);retain=checked(row['retain']);transport=checked(row['transport']);checked(row['late_copy'])
        require(pin(row['late_copy_cold_receipt']['path'])==row['late_copy_cold_receipt'] and row['late_copy_cold_receipt']['sha256']==row['late_copy']['sha256'],'late cold receipt differs')
        kept=checked(retain['retained_manifest'])
        controller=pin(ROOT/'controllers'/(label+'.json'));checked(controller)
        hot=ROOT/'measure/hot'/label
        def copied(name):
            path=hot/name;matches=[x['source'] for x in transport['files'] if capture(x['source']['path'])==capture(path) and x['verified'] is True]
            require(len(matches)==1,'unique copied source absent: '+str(path))
            require(pin(path)==matches[0],'copied source changed');return matches[0]
        scheduler=copied('stdout.jsonl');samplepin=copied('telemetry.jsonl')
        samples=[json.loads(line) for line in Path(samplepin['path']).read_text(encoding='utf-8').splitlines() if line.strip()]
        require(len(samples)>=2,'insufficient telemetry samples')
        require(len({x['pid'] for x in samples})==1,'telemetry process changed')
        elapsed=samples[-1]['at_unix']-samples[0]['at_unix'];cpu=samples[-1]['cpu_seconds']-samples[0]['cpu_seconds']
        require(elapsed>0 and cpu>=0,'telemetry interval invalid')
        cores=len(report['placement']['cpu_affinity'])
        telemetry.append(dict(case=label,source=samplepin,samples=len(samples),sampled_interval_seconds=elapsed,native_cpu_seconds_delta=cpu,average_native_cpu_cores=cpu/elapsed,affinity_cores=cores,percent_of_affinity_capacity=100*cpu/elapsed/cores,peak_sampled_rss_bytes=max(x['rss_bytes'] for x in samples),scope='Native process thread CPU time only, sampled first-to-last interval. Excludes initialization before first sample, final tail, archive and maintenance. No GPU or whole-host utilization claim.'))
        cases.append(dict(id=label,variant=label.split('-')[-1],report=row['report'],execution=report['execution'],archive=report['archive'],inspect=row['inspect'],transport=row['transport'],retain=row['retain'],late_copy=row['late_copy'],controller=controller,scheduler_stdout=scheduler,retained_manifest=retain['retained_manifest'],retained_root=str(Path(retain['retained_manifest']['path']).parent)))
        metadata.append(dict(case=label,coordinator_case_wall_seconds=row['coordinator_case_wall_seconds'],scope=row.get('coordinator_case_wall_scope','full dispatch through late receipt readback'),late_receipt_metadata_seconds=row['late_receipt_metadata_seconds'],late_copy_cold_receipt=row['late_copy_cold_receipt']))
    require(all(x==fingerprints[0] for x in fingerprints),'full fingerprint mismatch')
    require(not args.out.exists(),'preserve existing analysis attempt; choose fresh --out')
    args.out.mkdir(parents=True)
    manifest=args.out/'case-manifest.json';write(manifest,dict(sequential_nonoverlapping=True,cases=cases))
    helperpin=pin(HELPER);raw=args.out/'raw-summary.json';table=args.out/'summary-table.txt'
    with raw.open('xb') as stdout,table.open('xb') as stderr:
        result=subprocess.run([sys.executable,'-B',str(HELPER),'--root',str(ROOT),'--case-manifest',str(manifest)],stdout=stdout,stderr=stderr)
    require(result.returncode==0,'summary failed; preserve analysis outputs')
    summary=read(raw);require(summary['complete'] and summary['full_fingerprint_parity'],'summary incomplete')
    compactcases=[{k:v for k,v in c.items() if k!='fingerprint'} for c in summary['cases']]
    fullwall=(datetime.fromisoformat(state['finished_utc'])-datetime.fromisoformat(prior['started_utc'])).total_seconds()
    compact=dict(schema='training-speedups-full-analysis/v1',complete=True,full_fingerprint_parity=True,canonical_fingerprint_sha256=hashlib.sha256(json.dumps(fingerprints[0],sort_keys=True,separators=(',',':')).encode()).hexdigest(),completed_updates=648,completed_games=6480,dispatch_comparison=summary['dispatch_comparison'],complete_case_comparison=summary['complete_case_comparison'],cases=compactcases,native_telemetry=telemetry,coordinator_metadata=metadata,interruption_overhead={'scope':'Clean ABBA excludes contaminated first baseline and refused next dispatch; prior attempts preserved in formal-v2. No timing from those samples included.'},first_block_allocation_audit=state['first_block_reconciliation'],queue_waits=state.get('queue_waits',[]),wall_envelopes=dict(original_started_utc=prior['started_utc'],clean_started_utc=state['started_utc'],finished_utc=state['finished_utc'],overall_clean_coordinator_wall_seconds=fullwall,coordinator_wall_seconds=fullwall),provenance=dict(state=statepin,excluded_prior_attempt=pin(ROOT/'formal-v2/coordinator/state.json'),plan=state['plan'],helper=helperpin,analysis_script=pin(__file__),manifest=pin(manifest),raw_summary=pin(raw),table=pin(table)),accounting=summary['accounting'],claims='Two matched ABBA pairs compare combined changes and placement. Completed-phase totals exclude explicitly reported failed inspection/stopped gap/audit/self-copy overhead. Clean coordinator wall excludes separate prior contaminated/refused attempts. No individual patch causality, strength or holdout claim.')
    destination=args.out/'compact-analysis.json';write(destination,compact)
    print(json.dumps(dict(output=pin(destination),dispatch=compact['dispatch_comparison'],complete_case=compact['complete_case_comparison'])))

if __name__=='__main__': main()
