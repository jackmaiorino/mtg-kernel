"""Stage exact g115/V3 inputs for a destination; cannot launch evaluation.

Leaf artifacts keep their original bytes. Only descriptor paths and the V3
transfer receipt's destination build commit change. Both are recorded.
"""
import argparse,copy,hashlib,json,re,shutil
from pathlib import Path

G115='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1'
V3_IMPORT='9b5364158496c21adf02fcbf0c02aac5c5a94b424166c02c226e8f08f0779661'
V4_IMPORT='6c2fcb3730e23df685836527f69ef3c2092c0bd121d7aedfc26e54072c60e808'
def read(p):return json.loads(Path(p).read_bytes())
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def require(ok,message):
    if not ok:raise ValueError(message)
def checked(ref):
    path=Path(ref['path']);require(sha(path)==ref['sha256'],'Changed input: '+str(path));return path
def write(path,value):
    with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2)

def prepare(panel,root,destination,commit):
    require(re.fullmatch('[0-9a-f]{40}',commit) is not None,'Full source commit required')
    require(panel['candidate']['source']['checkpoint']['sha256']==G115,'Wrong lineage')
    require(panel['candidate']['source']['play_import']['sha256']==V4_IMPORT,'Changed initial lineage')
    require(panel['opponent']['source']['play_import']['sha256']==V3_IMPORT,'Changed frozen V3 import')
    root.mkdir();(root/'inputs').mkdir();originals=[];outputs=[]
    def reference(path):return dict(path=destination.rstrip('/')+'/inputs/'+path.name,sha256=sha(path))
    def leaf(ref):
        original=checked(ref);target=root/'inputs'/(ref['sha256']+original.suffix)
        if not target.exists():shutil.copyfile(original,target)
        require(sha(target)==ref['sha256'],'Copied leaf differs')
        originals.append(copy.deepcopy(ref));value=reference(target);outputs.append(value);return value
    candidate=copy.deepcopy(panel['candidate']);opponent=copy.deepcopy(panel['opponent'])
    descriptor=read(checked(candidate['source']['play_import']))
    require(set(descriptor)=={'schema','initialization','parameters'} and descriptor['schema']=='mtg-kernel-fresh-initialization-source/v1','Unexpected V4 source schema')
    for key in ('initialization','parameters'):descriptor[key]=leaf(descriptor[key])
    target=root/'inputs/g115-initialization-source.json';write(target,descriptor)
    candidate['source']['play_import']=reference(target);candidate['source']['checkpoint']=leaf(candidate['source']['checkpoint'])
    descriptor=read(checked(opponent['source']['play_import']))
    require(set(descriptor)=={'schema','source_checkpoint','source_registry','transfer_envelope','continuation_schedule'} and descriptor['schema']=='mtg-kernel-expanded-registry-transfer-source/v1','Unexpected V3 source schema')
    original_envelope_ref=copy.deepcopy(descriptor['transfer_envelope'])
    envelope=read(checked(original_envelope_ref));original_build=envelope['receipt']['destination_build_git_head']
    envelope['receipt']['destination_build_git_head']=commit
    target=root/'inputs/v3-transfer-envelope.json';write(target,envelope)
    descriptor['transfer_envelope']=reference(target)
    for key in ('source_checkpoint','source_registry','continuation_schedule'):descriptor[key]=leaf(descriptor[key])
    target=root/'inputs/v3-play-import-source.json';write(target,descriptor);opponent['source']['play_import']=reference(target)
    require(opponent['source']['checkpoint'] is None and opponent['v3_forced_actions'] and opponent['v3_spell_target_reference_adapter'],'Changed V3 adapters')
    result=dict(schema='g115-d3-portable-inputs/v1',launchable=False,source_commit=commit,
        destination_root=destination,candidate=candidate,opponent=opponent,originals=originals,
        v3_provenance_change=dict(original=original_envelope_ref,from_build=original_build,to_build=commit,
            only_changed_field='receipt.destination_build_git_head',new=descriptor['transfer_envelope']),
        leaf_outputs=outputs,scope='Exact bytes and path relocation only. No model change, experiment or allocation authority.')
    write(root/'sources.json',result)
    return result

def main():
    p=argparse.ArgumentParser();p.add_argument('--panel',type=Path,required=True);p.add_argument('--root',type=Path,required=True);p.add_argument('--destination',required=True);p.add_argument('--commit',required=True);a=p.parse_args()
    result=prepare(read(a.panel),a.root,a.destination,a.commit)
    write(a.root/'preparation.json',dict(panel=dict(path=str(a.panel),sha256=sha(a.panel)),helper=dict(path=str(Path(__file__).resolve()),sha256=sha(__file__)),sources_sha256=sha(a.root/'sources.json'),launchable=False))
    print(json.dumps(dict(root=str(a.root),leaf_files=len({r['path'] for r in result['leaf_outputs']}),launchable=False)))
if __name__=='__main__':main()
