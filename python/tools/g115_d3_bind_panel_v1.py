"""Bind all formal D3 inputs offline. This is not a launch authorization."""
import argparse,copy,hashlib,json
from pathlib import Path
from g115_d3_payload_v1 import prepare,read,require,sha,write
COMMIT='6604306cd67ccfb7e558b49b6504e033885fb0a8'
def bind(panel,accepted,proof,root,destination):
    require(panel['schema']=='g115-d3-panel-preparation/v1' and len(panel['jobs'])==2048,'Complete shared panel required')
    require(accepted['source']==COMMIT,'Accepted wrapper source differs')
    limits=accepted['search']
    require(all(limits[k]==v for k,v in dict(schema='mtg-kernel-v4-information-set-estimate-search/v3',algorithm='v4-depth-keyed-estimate-library-independent-chance/v3',root_allocation='round_robin',interior_bonus='prior_free',simulations=128,transitions=1024,depth=8,experiment_seed=20260922).items()),'Frozen search budget differs')
    require(proof['models'][0]['identity']['checkpoint_sha256']==panel['candidate']['source']['checkpoint']['sha256'],'g115 actual-model witness differs')
    models=[p['identity']['model'] for p in proof['models']]
    require(models[0]['weights_sha256']=='e2ca2f2b5dd750a59e24c71a4bac325ed7449d97b5892a79a80132e45d538333' and models[1]['weights_sha256']=='2fa88edf3c8f6170f721f6462b1297a70e71daaa94d7a36e43117f83d43453eb','Frozen weights differ')
    root.mkdir();(root/'requests').mkdir();sources=prepare(panel,root/'payload',destination.rstrip('/')+'/payload',COMMIT)
    jobs=[];coordinates=set()
    for job in panel['jobs']:
        coordinate=(job['cell'],job['replica'],job['candidate_seat'],job['arm']);require(coordinate not in coordinates,'Duplicate formal condition');coordinates.add(coordinate)
        command=copy.deepcopy(job['base_command']);seat=job['candidate_seat']
        require(command['sources'][seat]==panel['candidate'] and command['sources'][1-seat]==panel['opponent'],'Panel source differs')
        command['sources'][seat]=copy.deepcopy(sources['candidate']);command['sources'][1-seat]=copy.deepcopy(sources['opponent'])
        require(command['capture_decisions'] is False,'Production capture mode differs')
        if job['arm']=='search':command['sources'][seat]=dict(kind='information_set_search_v3',source=command['sources'][seat]['source'],descriptor=limits)
        else:require(job['arm']=='baseline','Unknown arm')
        command['output_directory']=destination.rstrip('/')+'/outputs/'+job['id']
        path=root/'requests'/(job['id']+'.json');write(path,command)
        # Exhaustive shape check: only source binding and output placement change.
        before={k:v for k,v in job['base_command'].items() if k not in ('sources','output_directory')}
        after={k:v for k,v in command.items() if k not in ('sources','output_directory')}
        require(before==after,'Match, decks, sideboards or other native option changed')
        jobs.append(dict(id=job['id'],request=dict(path=str(path),sha256=sha(path)),output_directory=str(root/'outputs'/job['id']),native_request=destination.rstrip('/')+'/requests/'+path.name,native_output_directory=command['output_directory']))
    expected={(c,r,s,a) for c in range(64) for r in range(8) for s in (0,1) for a in ('baseline','search')}
    require(coordinates==expected,'Formal panel incomplete')
    result=dict(schema='g115-d3-bound-panel/v1',launchable=False,source_commit=COMMIT,models=models,jobs=jobs,
        destination=destination,review_verdict=None,compute_choice=None,
        scope='All2048 fixed input bindings only. Guarded launch must add design disposition, compatible fleet qualification and runtime pins.')
    write(root/'execution.json',result);return result
def main():
    p=argparse.ArgumentParser();p.add_argument('--panel',type=Path,required=True);p.add_argument('--accepted-wrapper',type=Path,required=True);p.add_argument('--model-witness',type=Path,required=True);p.add_argument('--root',type=Path,required=True);p.add_argument('--destination',required=True);a=p.parse_args()
    result=bind(read(a.panel),read(a.accepted_wrapper),read(a.model_witness),a.root,a.destination)
    write(a.root/'preparation.json',dict(inputs={k:dict(path=str(v),sha256=sha(v)) for k,v in [('panel',a.panel),('accepted_wrapper',a.accepted_wrapper),('model_witness',a.model_witness),('binder',Path(__file__))]},execution_sha256=sha(a.root/'execution.json'),launchable=False))
    print(json.dumps(dict(jobs=len(result['jobs']),launchable=False,execution_sha256=sha(a.root/'execution.json'))))
if __name__=='__main__':main()
