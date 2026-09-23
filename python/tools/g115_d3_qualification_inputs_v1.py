"""Freeze a balanced consumed cohort, disjoint from D3, for CPU qualification."""
import argparse,copy,json
from pathlib import Path
from g115_d3_payload_v1 import prepare,sha,read,require,write
COMMIT='e258daf3ab807cd6d8616a1431a21ea5ee22ac0b'
def main():
    p=argparse.ArgumentParser();p.add_argument('--archive-plan',type=Path,required=True);p.add_argument('--panel',type=Path,required=True);p.add_argument('--accepted-wrapper',type=Path,required=True);p.add_argument('--root',type=Path,required=True);p.add_argument('--host',choices=['jack','haleyspc','runpod'],required=True);p.add_argument('--runtime',type=Path,required=True);p.add_argument('--destination',required=True);p.add_argument('--workers',type=int,nargs='+',required=True);p.add_argument('--lease-name');p.add_argument('--inventory',type=Path,required=True);a=p.parse_args()
    panel=read(a.panel);archive=read(a.archive_plan);accepted=read(a.accepted_wrapper);runtime=read(a.runtime)
    require(accepted['source']==COMMIT,'Unaccepted wrapper source')
    a.root.mkdir();payload=prepare(panel,a.root/'payload',a.destination.rstrip('/')+'/payload',COMMIT)
    selected=[]
    # Two opponents per deck supply 32 search jobs, so a 32-worker host can
    # actually exercise its capacity. The original cohort is a strict subset.
    for i,offset in ((i,offset) for offset in (1,2) for i in range(8)):
        for seat in (0,1):
            identifier=f'pair-{i}-{(i+offset)%8}-p{seat}'
            found=[j for j in archive['jobs'] if j['id']==identifier];require(len(found)==1,'Consumed cohort absent')
            old=found[0];require(old['seat']==seat,'Candidate seat mismatch')
            for arm in ('baseline','search'):
                command=copy.deepcopy(old['command']);command['sources'][seat]=copy.deepcopy(payload['candidate']);command['sources'][1-seat]=copy.deepcopy(payload['opponent'])
                if arm=='search':command['sources'][seat]=dict(kind='information_set_search_v3',source=command['sources'][seat]['source'],descriptor=accepted['search'])
                command['output_directory']='UNBOUND';require(command['capture_decisions'] is False,'Production capture setting differs')
                selected.append(dict(id=identifier+'-'+arm,arm=arm,candidate_seat=seat,command=command))
    spec=dict(schema='g115-d3-throughput-qualification/v1',formal_measurement=False,source_commit=COMMIT,host=a.host,
        jobs=selected,worker_counts=a.workers,runtime_files=runtime['runtime_files'],command_prefix=runtime['command_prefix'],
        formal_panel_seeds=[v for row in panel['seeds'] for v in row],job_timeout_seconds=1800,group_timeout_seconds=9000,
        reserve_bytes=(1 if a.host=='runpod' else 32)*2**30,
        guard_directory='/run/phase1/'+a.lease_name if a.lease_name else None,lease_name=a.lease_name,inventory=read(a.inventory),
        provenance={k:dict(path=str(v),sha256=sha(v)) for k,v in [('archive_plan',a.archive_plan),('panel',a.panel),('accepted_wrapper',a.accepted_wrapper),('runtime',a.runtime)]})
    require(a.host!='runpod' or a.lease_name,'Paid qualification requires exact lease name')
    write(a.root/'spec.json',spec);print(json.dumps(dict(jobs=len(selected),root=str(a.root),formal_measurement=False)))
if __name__=='__main__':main()
