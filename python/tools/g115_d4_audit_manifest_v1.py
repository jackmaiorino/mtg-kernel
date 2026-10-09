"""Build the fixed D4 2400/800-update archived-audit manifests, without execution."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import time
import zipfile
from g115_d4_audit_pipeline_v1 import limits, signal_limits, REMOTE_ENDPOINTS

B = Path('E:/mtg-g115-lineage-20260923')
REMOTE = 'C:/mtg-node/g115-d4-audit-production-20260925-001'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def pin(path):
    with Path(path).open('rb') as stream:
        return {'path': str(path), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}


def read(ref):
    raw = Path(ref['path']).read_bytes()
    require(digest(raw) == ref['sha256'], 'Pinned metadata changed')
    return json.loads(raw)


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)


def order(value, keys):
    return {k:value[k] for k in keys if k in value}


def canonical_source(source):
    return {'play_import': order(source['play_import'], ['path','sha256']),
            'feature_transfer': order(source['feature_transfer'], ['expected_feature_contract_digest','expected_feature_encoding_digest']),
            'checkpoint': order(source['checkpoint'], ['path','sha256']) if source.get('checkpoint') else None}


def canonical_episode(episode):
    # order() drops unknown keys; opt-in opponent fields must not vanish from a manifest.
    assert not {'opponent_search', 'opponent_kind'} & episode.keys()
    value = order(episode, ['id','seed','starting_player','learner_seat','opponent','registered','selected','postboard','max_physical_decisions','max_policy_steps'])
    if value.get('opponent') is None:
        value.pop('opponent', None)
    else:
        value['opponent'] = canonical_source(value['opponent'])
    for k in ['registered','selected']:
        value[k] = [order(d, ['label','mainboard','sideboard']) for d in value[k]]
    return value


def canonical_config(config):
    value = order(config, ['source','updates','inputs_enabled','learning_rate','value_coefficient','gamma','lambda','gpu_ordinal','max_chunk_substeps','projection_mode','entropy_coefficient'])
    value['source'] = canonical_source(value['source'])
    value['updates'] = [[canonical_episode(e) for e in batch] for batch in value['updates']]
    if value.get('projection_mode', 'all') == 'all':
        value.pop('projection_mode', None)
    if value.get('entropy_coefficient', 0) == 0:
        value.pop('entropy_coefficient', None)
    return json.dumps(value, separators=(',', ':'), ensure_ascii=False).encode()


def build(output):
    output.mkdir()
    started = time.monotonic()
    records = read({'path': str(B/'d4-continuation-records-003/analysis.json'),
                    'sha256': '24c9c199182d458d540d15545422a945970df84f3975b0ab70ff7c544587f1e3'})
    old_portable = B/'d4-replay-computehost-preparation-001/payload'
    old_remote = 'C:/mtg-node/g115-d4-replay-timing-20260925-001'
    relocation_map = {}
    for item in json.loads((old_portable/'relocations.json').read_bytes()):
        if 'original' in item:
            portable = item['portable']
            require(pin(old_portable/portable['path'].removeprefix(old_remote+'/'))['sha256'] == portable['sha256'], 'Prior portable file changed')
            relocation_map[item['original']['sha256']] = portable
    def relocate(value):
        if isinstance(value, dict):
            if set(value) == {'path','sha256'}:
                require(value['sha256'] in relocation_map, 'Source not covered by verified relocation ledger')
                return relocation_map[value['sha256']]
            return {k:relocate(v) for k,v in value.items()}
        return copy.deepcopy(value)
    report = {'hosts': [], 'archive_transfers': {}, 'checkpoint_relocations': []}
    snapshots = json.loads((B/'d4-signal-batch-001/analysis.json').read_bytes())['source_snapshots']
    for host in ['desktop', 'computehost']:
        folder = output/host
        folder.mkdir()
        def host_pin(path):
            ref = pin(path)
            if host == 'computehost':
                ref['path'] = REMOTE+'/packet/'+path.relative_to(folder).as_posix()
            return ref
        def imported(ref):
            target = folder/'inputs'/ref['sha256']/Path(ref['path']).name
            if not target.exists():
                require(pin(ref['path'])['sha256'] == ref['sha256'], 'Import changed')
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ref['path'], target)
            return host_pin(target)
        action_sources = {'policy':snapshots[0], 'tensorizer':snapshots[1]}
        if host == 'computehost':
            action_sources = {k:imported(v) for k,v in action_sources.items()}
        groups, commands, signals, templates = [], [], [], []
        archive_cache = {}
        metadata_files = {}
        metadata_archive = folder/'checkpoint-metadata.zip'
        metadata_zip = zipfile.ZipFile(metadata_archive, 'x', compression=zipfile.ZIP_DEFLATED) if host == 'computehost' else None
        selected = [r for r in records['runs'] if (r['endpoint'] in REMOTE_ENDPOINTS) == (host == 'computehost')]
        for run in selected:
            endpoint = run['endpoint']
            config = read(run['config']);original_config_hash = digest(canonical_config(config))
            config_ref = run['config']
            if host == 'computehost':
                config = copy.deepcopy(config);config['source'] = relocate(config['source'])
                save(folder/'configs'/f'{endpoint}.json', config)
                config_ref = host_pin(folder/'configs'/f'{endpoint}.json')
            config_hash = digest(canonical_config(config))
            mapping = run['mapping'];archived = mapping['mode'] == 'sharded_zip'
            members = {};handles = []
            if archived:
                key = mapping['archive_index']['sha256']
                if key not in archive_cache:
                    index = read(mapping['archive_index'])
                    original_index = copy.deepcopy(index)
                    for shard in index['shards']:
                        original = shard['archive']
                        handle = zipfile.ZipFile(original['path']);handles.append(handle)
                        for name, sha in shard['files'].items():
                            info = handle.getinfo(name)
                            members[name] = (handle, info.file_size, sha, original)
                        if host == 'computehost':
                            destination = REMOTE+'/archives/'+original['sha256']+'/'+Path(original['path']).name
                            report['archive_transfers'][original['sha256']] = {'source':original, 'destination':destination, 'bytes':Path(original['path']).stat().st_size}
                            shard['archive'] = {'path':destination, 'sha256':original['sha256']}
                    index_ref = mapping['archive_index']
                    if host == 'computehost':
                        save(folder/'indexes'/f'{key}.json', index)
                        index_ref = host_pin(folder/'indexes'/f'{key}.json')
                    archive_cache[key] = (members, handles, index_ref, index)
                members, handles, index_ref, index = archive_cache[key]
            cp_refs = []
            trajectory_hashes = {t['relative_path']:t['sha256'] for t in run['trajectory_index']}
            for update in range(200):
                group_name = f'{endpoint}-{update:04}'
                files = []
                refs = {}
                checkpoint = None
                for name in ['checkpoint.json','optimizer.json']+[f'episode-{s:03}.json' for s in range(10)]:
                    relative = f'{update:04}/{name}'
                    if archived:
                        member = mapping['member_prefix']+relative
                        z, length, sha, original_archive = members[member]
                        archive_ref = next(s['archive'] for s in index['shards'] if s['archive']['sha256'] == original_archive['sha256'])
                        entry = {'name':name,'bytes':length,'sha256':sha,'member':member,'archive':archive_ref,'index':index_ref}
                        if name == 'checkpoint.json':
                            raw = z.read(member);require(digest(raw) == sha, 'Checkpoint member changed');checkpoint = json.loads(raw)
                            require(checkpoint['config_sha256'] == original_config_hash and checkpoint['next_update'] == update+1, 'Checkpoint/config/update differs')
                            if host == 'computehost':
                                original_sha = sha
                                checkpoint['config_sha256'] = config_hash
                                raw = (json.dumps(checkpoint, separators=(',', ':'))+'\n').encode()
                                member = group_name+'/checkpoint.json';metadata_zip.writestr(member, raw)
                                sha = digest(raw);metadata_files[member] = sha
                                entry = {'name':name,'bytes':len(raw),'sha256':sha,'member':member,'portable_metadata':True}
                                report['checkpoint_relocations'].append({'endpoint':endpoint,'update':update,'original_sha256':original_sha,'portable_sha256':sha,'original_config_sha256':original_config_hash,'portable_config_sha256':config_hash,'optimizer_sha256':checkpoint['optimizer_sha256']})
                        files.append(entry)
                        ref = {'path':f'@stage/{group_name}/{name}','sha256':sha}
                    else:
                        path = Path(mapping['output_directory'])/relative
                        if name == 'checkpoint.json':
                            ref = pin(path);checkpoint = read(ref)
                            require(checkpoint['config_sha256'] == config_hash and checkpoint['next_update'] == update+1, 'Direct checkpoint differs')
                        elif name == 'optimizer.json':
                            ref = {'path':str(path),'sha256':checkpoint['optimizer_sha256']}
                        else:
                            ref = {'path':str(path),'sha256':trajectory_hashes[relative]}
                    refs[name] = ref
                require(refs['optimizer.json']['sha256'] == checkpoint['optimizer_sha256'], 'Optimizer digest differs')
                trajectories = [refs[f'episode-{s:03}.json'] for s in range(10)]
                require([r['sha256'] for r in trajectories] == checkpoint['trajectory_sha256'] == [trajectory_hashes[f'{update:04}/episode-{s:03}.json'] for s in range(10)], 'Trajectory/receipt order differs')
                if archived:groups.append({'name':group_name,'files':files})
                cp_refs.append(refs['checkpoint.json'])
                models = [{'kind':'parent','label':'g115','source':config['source']}]
                for u,label in ([(update-1,'before')] if update else [])+[(update,'after')]:
                    models.append({'kind':'checkpoint','label':label,'config':config_ref,'checkpoint':cp_refs[u]})
                receipt_path = folder/'receipts'/f'{group_name}.json';save(receipt_path,run['receipts'][update])
                command = {'models':models,'trajectories':trajectories,'output_directory':'owner-overrides','workers':1,'max_choice_rows_per_trajectory':16,'minimum_behavior_replay_rows':1}
                signal = {'schema':'g115-d4-learning-signal-command/v1','endpoint':endpoint,'update':update,'config':config_ref,'receipt':host_pin(receipt_path),'trajectories':trajectories,'action_sources':action_sources,'output':'owner-overrides'}
                cp = folder/'commands'/f'{group_name}.json';sp = folder/'signals'/f'{group_name}.json'
                save(cp,command);save(sp,signal);commands.append(host_pin(cp));signals.append(host_pin(sp));templates.append(command)
        for members, handles, _, _ in archive_cache.values():
            for handle in handles:handle.close()
        if host == 'desktop':
            for arm in ['control','broader']:
                root = Path('E:/mtg-postboard-campaign-20260920/broader-exposure-pilot-001')/arm
                config_ref = pin(root/'config.json');config = read(config_ref)
                done = json.loads((root/'run/completion.json').read_bytes());previous = None
                require(len(done['iterations']) == 200, 'Native pilot horizon differs')
                for update, iteration in enumerate(done['iterations']):
                    receipt_ref = read(iteration)['update'];receipt = read(receipt_ref)
                    models = [{'kind':'native_expanded','label':'g115','source':config['initial_source']}]
                    for ref,label in ([(previous,'before')] if previous else [])+[(receipt['checkpoint'],'after')]:
                        source = copy.deepcopy(config['initial_source']);source['checkpoint'] = ref
                        models.append({'kind':'native_expanded','label':label,'source':source})
                    previous = receipt['checkpoint'];name = f'pilot-{arm}-{update:04}'
                    command = {'trajectory_kind':'native_expanded_v3','models':models,'trajectories':receipt['trajectories'],'output_directory':'owner-overrides','workers':1,'max_choice_rows_per_trajectory':16,'minimum_behavior_replay_rows':1}
                    signal = {'schema':'g115-d4-learning-signal-command/v1','endpoint':'pilot-'+arm,'update':update,'config':config_ref,'receipt':receipt_ref,'trajectories':receipt['trajectories'],'action_sources':action_sources,'output':'owner-overrides'}
                    cp = folder/'commands'/f'{name}.json';sp = folder/'signals'/f'{name}.json'
                    save(cp,command);save(sp,signal);commands.append(host_pin(cp));signals.append(host_pin(sp));templates.append(command)
        else:
            metadata_zip.close();archive_ref = host_pin(metadata_archive)
            save(folder/'checkpoint-index.json',{'shards':[{'archive':archive_ref,'files':metadata_files}]})
            for group in groups:
                for file in group['files']:
                    if file.pop('portable_metadata',False):file.update(archive=archive_ref,index=host_pin(folder/'checkpoint-index.json'))
        work = {'groups':groups,'commands':commands,'signals':signals};save(folder/'workload.json',work)
        plan = json.loads((B/'d4-audit-signal-plan-001'/host/'manifest.json').read_bytes())
        evidence = B/'d4-audit-signal-cross-host-001'
        for field,name in [('throughput',host+'-throughput.json'),('allocation','allocation-selection.json'),('cross_host_signal_verification','verification.json')]:
            plan[field] = pin(evidence/name) if host=='desktop' else imported(pin(evidence/name))
        plan.update(mode='production',workers=[8],total_seconds=21600,cap_bytes=60_000_000_000 if host=='desktop' else 40_000_000_000,
                    workload=host_pin(folder/'workload.json'),worker_root=str(B/'d4-audit-production-desktop-worker-001') if host=='desktop' else REMOTE+'/worker',
                    retained_roots=[str(folder)] if host=='desktop' else [REMOTE+'/packet',REMOTE+'/archives'],
                    scope='Full archived audit, all3200 updates across two shards; no games/training;16 systematic choice rows per trajectory maximum plus all-record signal scans.')
        limits(plan,work,templates)
        if host=='desktop':signal_limits(plan,work,templates,host)
        save(folder/'manifest.json',plan)
        save(folder/'file-hashes.json',{p.relative_to(folder).as_posix():pin(p)['sha256'] for p in folder.rglob('*') if p.is_file()})
        report['hosts'].append({'host':host,'manifest':host_pin(folder/'manifest.json'),'commands':len(commands),'signals':len(signals),'groups':len(groups),'input_bytes':sum(f['bytes'] for g in groups for f in g['files']),'payload_bytes':sum(p.stat().st_size for p in folder.rglob('*') if p.is_file()),'native_started':False})
    report.update(complete=True,seconds=time.monotonic()-started,builder=pin(__file__),archive_bytes=sum(x['bytes'] for x in report['archive_transfers'].values()))
    save(output/'preparation.json',report)
    print(json.dumps({k:v for k,v in report.items() if k not in ('archive_transfers','checkpoint_relocations')}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',required=True)
    build(Path(parser.parse_args().output))
