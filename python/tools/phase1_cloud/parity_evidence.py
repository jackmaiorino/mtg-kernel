"""Read actual native artifacts for training/resume and complete BO3 parity.

Only explicit file-location mappings are supported. Pinned contents are never
rewritten, and reported equality is recomputed from full artifacts rather than
accepting an earlier boolean or summary label. No native or provider execution.
"""
from __future__ import annotations
import copy
import hashlib
from pathlib import Path
import re
from common import encoded, pin, read, require
from semantic import trajectory_digest, validated_map
from fresh_source import (SOURCE_SCHEMA, inspect_source, initial_state, is_fresh_origin, origin_key,
                          validate_checkpoint_origin, validate_inference_identity, validate_trajectory)
from throughput import (canonical_training_config, training_contract, preparation_workers,
                        validate_preparation_runner, validate_preparation_update, validate_preparation_result)

STATE_KEYS={'schema','feature_contract_digest','feature_encoding_digest','card_db_hash',
            'source_import','state_sha256','adam_step','scorer_bias_anchor_bits','parameters',
            'first_moments','second_moments','trajectories','loss_identity',
            'learning_rate_bits','value_coefficient_bits'}
ALL_TRAJECTORY_SCHEMA='phase1-complete-training-trajectory-semantics/v2'


def digest(value):return hashlib.sha256(encoded(value)).hexdigest()


def normal(path):
    require(isinstance(path,str) and path,'nonempty artifact path required')
    value=path.replace('\\','/').rstrip('/')
    require('..' not in value.split('/'),'artifact path traversal rejected')
    return value


class Artifacts:
    def __init__(self,path_mappings=()):
        self.mappings=[];self.used={}
        for item in path_mappings:
            require(set(item)=={'from','to'},'unsupported artifact path mapping')
            source=normal(item['from'])
            require(source.startswith('/') or re.match(r'^[A-Za-z]:(/|$)',source),
                    'absolute artifact mapping source required')
            self.mappings.append((source,Path(item['to']).resolve()))
        require(len({item[0] for item in self.mappings})==len(self.mappings),'ambiguous artifact mapping')
        self.mappings.sort(key=lambda row:len(row[0]),reverse=True)

    def resolve(self,path):
        value=normal(path)
        for source,target in self.mappings:
            if value==source or value.startswith(source+'/'):
                result=(target/value[len(source):].lstrip('/')).resolve()
                require(result.is_relative_to(target),'mapped artifact escapes local root')
                return result
        return Path(path)

    def local_pin(self,reference):
        actual=pin(self.resolve(reference['path']),reference['sha256'])
        self.used[(reference['path'],reference['sha256'])]=actual
        return actual

    def checked(self,reference):return read(self.local_pin(reference)['path'])

    def expected(self,reference,path):
        require(normal(reference['path'])==normal(path),'artifact belongs to a different native run')
        return self.checked(reference)


def source_build(reader,reference):
    value=reader.checked(reference)
    require(re.fullmatch('[0-9a-f]{40}',value['source_commit']),'full source build commit required')
    binaries=value.get('binaries',{})
    require('native_expanded_training_run_v1' in binaries and 'learned_sideboard_v1' in binaries,
            'trainer and BO3 executable pins required')
    for item in binaries.values():reader.local_pin(item)
    for field in ('source_archive','toolchain'):
        require(field in value,'source archive and toolchain pins required')
        reader.local_pin(value[field])
    return {'source_commit':value['source_commit'],
            'binaries':{name:item['sha256'] for name,item in binaries.items()},
            'source_archive_sha256':value['source_archive']['sha256'],
            'toolchain_sha256':value['toolchain']['sha256']}


def original_config(reader,config,payload=None):
    value=copy.deepcopy(config)
    rows=None
    if payload:
        rows=validated_map(reader.local_pin(payload),config)
        def original(source):
            found=[row['original'] for row in rows if row['relocated']==source]
            require(found and all(item==found[0] for item in found),'ambiguous source relocation')
            return copy.deepcopy(found[0])
        value['initial_source']=original(value['initial_source'])
        for opponent in value['opponents']:opponent['source']=original(opponent['source'])
    return value,rows


def checkpoint_state(reader,reference,trajectories=None):
    value=reader.checked(reference)
    require(set(value)==STATE_KEYS,'unsupported checkpoint state fields')
    validate_checkpoint_origin(value)
    require(type(value['adam_step']) is int and value['adam_step']>=0,'invalid Adam step')
    require(len(value['parameters'])>0 and len(value['parameters'])==len(value['first_moments'])==
            len(value['second_moments']),'optimizer state dimensions differ')
    if trajectories is not None:
        require(value['trajectories']==trajectories,'checkpoint was updated from different trajectories')
    # All trajectory bytes are checked independently. Their path-bearing file
    # provenance is not optimizer state and legitimately differs after restart.
    if trajectories is not None:
        for item in value['trajectories']:reader.local_pin(item)
    state={key:item for key,item in value.items() if key!='trajectories'}
    return value,digest(state)


def initial_training_state(reader,config,inspection=None,relocations=None):
    source=config['initial_source']
    if source.get('checkpoint') is not None:
        return checkpoint_state(reader,source['checkpoint'])
    require(inspection is not None,'fresh Adam0 source requires actual native inspection evidence')
    equivalents=[row['original'] for row in (relocations or []) if row['relocated']==source]
    proof=initial_state(source,inspection,resolve_pin=lambda ref:reader.local_pin(ref)['path'],
                        equivalent_sources=equivalents)
    return proof,digest(proof)


def scheduled_opponent(config,index,choice,checkpoints):
    current=copy.deepcopy(config['initial_source'])
    if index:current['checkpoint']=checkpoints[index-1]
    if choice=={'kind':'current'}:return current
    if choice=={'kind':'initial'}:return copy.deepcopy(config['initial_source'])
    if set(choice)=={'kind','id'} and choice['kind']=='fixed':
        found=[row['source'] for row in config['opponents'] if row['id']==choice['id']]
        require(len(found)==1,'fixed opponent missing or ambiguous');return copy.deepcopy(found[0])
    require(set(choice)=={'kind','index'} and choice['kind']=='completed_iteration' and
            type(choice['index']) is int and 0<=choice['index']<index,'invalid prior checkpoint schedule choice')
    result=copy.deepcopy(config['initial_source']);result['checkpoint']=checkpoints[choice['index']]
    return result


def complete_trajectory_digest(document,config,canonical,iteration,current_source,previous_state,
                               previous_state_digest,history=None):
    """Normalize only already verified model and generated-checkpoint provenance.

    The caller has rehashed the previous full checkpoint and validated the exact
    expected learner/opponent sources for this episode. Here the identity's raw
    checkpoint SHA, state and Adam cursor must also agree before normalization.
    All other JSON fields, including arbitrary additional paths/hashes, survive.
    """
    validate_trajectory(document)
    value=copy.deepcopy(document);changes=[]
    require(len(value['seat_behaviors'])==2,'trajectory requires both seat behaviors')
    if history is None:
        history=[(current_source,previous_state,previous_state_digest,iteration-1)] if iteration else []
    def normalized_source(source,field):
        matches=[row for row in history if source==row[0]]
        if matches:
            require(len(matches)==1,'ambiguous generated checkpoint source')
            _,saved,saved_digest,prior_index=matches[0]
            require(0<=prior_index<iteration,'unverified future checkpoint source')
            generated={'generated_checkpoint_iteration':prior_index,'full_state_payload_sha256':saved_digest}
            require({'path','sha256'}<=set(source['checkpoint'])<={'path','sha256','bytes'},
                    'generated checkpoint reference contains unsupported fields')
            result=copy.deepcopy(canonical['initial_source']);result['checkpoint']=generated
            changes.extend(field+'.checkpoint.'+key for key in sorted(source['checkpoint']))
            if result['play_import']!=source['play_import']:changes.append(field+'.play_import')
            return result,(saved,saved_digest)
        originals=[(config['initial_source'],canonical['initial_source']),
            *[(a['source'],b['source']) for a,b in zip(config['opponents'],canonical['opponents'])]]
        matches=[b for a,b in originals if a==source]
        require(matches and all(item==matches[0] for item in matches),'unproven static model source')
        result=copy.deepcopy(matches[0])
        for key in ('play_import','checkpoint'):
            if result.get(key)!=source.get(key):changes.append(field+'.'+key)
        return result,None
    for seat,behavior in enumerate(value['seat_behaviors']):
        raw_source=behavior['source'];identity=behavior['identity']
        require(identity.get('checkpoint_sha256')==(raw_source.get('checkpoint') or {}).get('sha256'),
                'trajectory identity does not bind its exact checkpoint file')
        behavior['source'],is_generated=normalized_source(raw_source,f'seat_behaviors[{seat}].source')
        if is_generated is not None:
            saved,saved_digest=is_generated
            require(identity['state_sha256']==saved['state_sha256'] and
                    identity['adam_step']==saved['adam_step'],
                    'generated checkpoint identity has a different state or Adam cursor')
            if 'source_import' in identity:
                require(identity['source_import']==saved['source_import'],'generated checkpoint origin differs')
            identity['checkpoint_sha256']=saved_digest
            changes.append(f'seat_behaviors[{seat}].identity.checkpoint_sha256')
    value['episode']['opponent'],_=normalized_source(value['episode']['opponent'],'episode.opponent')
    return digest(value),sorted(changes)


def training_run(reader,spec):
    config=reader.checked(spec['config']);root=config['output_directory'].rstrip('/\\')
    build=source_build(reader,spec['source_build'])
    start=reader.expected(spec['run_start'],root+'/run.json')
    require(start['config']==canonical_training_config(config),'native start config differs')
    validate_preparation_runner(config,start)
    require(start['git_commit']==build['source_commit'] and start['device']=='cpu' and
            start['gpu_ordinal'] is None,'native start source/device differs')
    require(start.get('collection_workers_requested',1)==config.get('collection_workers',1),
            'native collector worker setting differs')
    completion=reader.expected(spec['completion'],root+'/completion.json')
    validate_preparation_runner(config,completion)
    count=len(config['iterations'])
    require(count>0 and completion['complete'] is True and completion['completed_iterations']==count and
            completion['planned_iterations']==count and len(completion['iterations'])==count,
            'native full run incomplete')
    require(completion['device']=='cpu' and completion['gpu_ordinal'] is None,'completion device differs')
    canonical,relocations=original_config(reader,config,spec.get('payload'))
    inspection=spec.get('initial_inspection')
    if inspection is None and spec.get('payload'):
        inspection=reader.checked(spec['payload']).get('initial_inspection')
    initial,initial_digest=initial_training_state(reader,config,inspection,relocations)
    source_info={};known_states={};origins={}
    def remember(source,saved):known_states[digest(source)]=saved
    remember(config['initial_source'],initial)
    initial_origin=initial['source_import']
    for source in [config['initial_source'],*[item['source'] for item in config['opponents']]]:
        reader.local_pin(source['play_import'])
        descriptor=reader.checked(source['play_import'])
        if descriptor.get('schema')==SOURCE_SCHEMA:
            info=inspect_source(source['play_import'],source['feature_transfer'],
                                resolve_pin=lambda ref:reader.local_pin(ref)['path'])
            source_info[digest(source)]=info
            origins[origin_key(source)]=info['origin']
            if source.get('checkpoint'):
                saved,_=checkpoint_state(reader,source['checkpoint'])
                require(saved['source_import']==info['origin'],'configured checkpoint/descriptor origin differs')
                remember(source,saved)
        elif source.get('checkpoint'):reader.local_pin(source['checkpoint'])
    state=initial['state_sha256'];state_hashes=[];states=[];checkpoints=[];first=[];first_raw=[];collections=[]
    all_semantic=[];all_pins=[];normalizations=[];previous_state=initial;previous_digest=initial_digest
    preparation=[];history=[]
    for index,reference in enumerate(completion['iterations']):
        receipt=reader.expected(reference,root+f'/iterations/{index:06d}/complete.json')
        require(receipt['schema']=='mtg-kernel-native-expanded-iteration/v1' and receipt['iteration']==index,
                'native iteration schema/order differs')
        collection=reader.checked(receipt['collection']);update=reader.checked(receipt['update'])
        execution=validate_preparation_update(config,update)
        if execution is not None:preparation.append({'iteration':index,'update':receipt['update'],**execution})
        require(collection['schema']=='mtg-kernel-expanded-deck-collection/v1' and
                update['schema']=='mtg-kernel-expanded-deck-update/v1' and
                collection['complete'] is True and update['complete'] is True,'native phase incomplete or wrong schema')
        current_source=copy.deepcopy(config['initial_source'])
        if index:current_source['checkpoint']=checkpoints[-1]
        require(receipt['source']==current_source,'native iteration did not use the preceding learner checkpoint')
        require(update['before_state_sha256']==state and collection['behavior_state_sha256']==state,
                'learner collected or replayed stale behavior state')
        trajectories=collection['trajectories']
        require(len(trajectories)==len(config['iterations'][index]['episodes'])==10,
                'update does not contain ten complete fresh trajectories')
        saved,state_digest=checkpoint_state(reader,update['checkpoint'],trajectories)
        # A generated checkpoint's recorded ancestry is bound to the qualified
        # initial origin, never accepted on its own self-consistent claim.
        require(saved['source_import']==initial_origin,'generated checkpoint origin differs from the qualified initial source')
        require(saved['state_sha256']==update['after_state_sha256']==receipt['output_identity']['state_sha256'] and
                saved['adam_step']==initial['adam_step']+index+1,'native state/Adam chain differs')
        batch_semantic=[];batch_changes=[]
        for offset,item in enumerate(trajectories):
            trajectory=reader.checked(item)
            fresh=validate_trajectory(trajectory)
            require(trajectory['behavior_state_sha256']==state,'trajectory behavior is stale')
            requested=config['iterations'][index]['episodes'][offset]['episode']
            require(all(trajectory['episode'].get(key)==value for key,value in requested.items()),
                    'trajectory seed/registration/episode contract differs')
            choice=config['iterations'][index]['episodes'][offset]['opponent']
            expected_opponent=scheduled_opponent(config,index,choice,checkpoints)
            seat=requested['learner_seat']
            require(len(trajectory['seat_behaviors'])==2 and trajectory['episode']['opponent']==expected_opponent and
                    trajectory['seat_behaviors'][seat]['source']==current_source and
                    trajectory['seat_behaviors'][1-seat]['source']==expected_opponent,
                    'trajectory learner/opponent source differs from the current configured policies')
            if fresh:
                for behavior in trajectory['seat_behaviors']:
                    raw_source=behavior['source'];identity=behavior['identity'];key=digest(raw_source)
                    expected_state=known_states.get(key)
                    info=source_info.get(key)
                    if expected_state is not None:
                        validate_inference_identity(identity,raw_source,expected_state['source_import'])
                        require(identity['state_sha256']==expected_state['state_sha256'] and
                                identity['adam_step']==expected_state['adam_step'],'recorded actor state/Adam differs')
                    elif info is not None:
                        validate_inference_identity(identity,raw_source,info['origin'])
                        require(raw_source['checkpoint'] is None and identity['adam_step']==0 and
                                identity['state_sha256']==info['initial_state_sha256'] and
                                identity['model']==info['initial_model'],'frozen fresh Adam0 actor differs')
                    else:
                        validate_inference_identity(identity,raw_source)
                        require(not is_fresh_origin(identity['source_import']),'unproven fresh source origin')
            if index==0:
                first.append(trajectory_digest(trajectory,config,relocations,origins));first_raw.append(item['sha256'])
            semantic,changed=complete_trajectory_digest(trajectory,config,canonical,index,current_source,
                                                       previous_state,previous_digest,history)
            batch_semantic.append(semantic)
            if changed:batch_changes.append({'episode_offset':offset,'fields':changed})
        state=saved['state_sha256'];states.append(state);state_hashes.append(state_digest)
        all_semantic.append(batch_semantic);all_pins.append(trajectories)
        if batch_changes:normalizations.append({'iteration':index,'episodes':batch_changes})
        previous_state=saved;previous_digest=state_digest
        checkpoints.append(update['checkpoint'])
        next_source=copy.deepcopy(config['initial_source']);next_source['checkpoint']=update['checkpoint']
        remember(next_source,saved);history.append((next_source,saved,state_digest,index))
        collections.append(receipt['collection'])
    require(state==completion['actual_identity']['state_sha256'] and
            completion['actual_identity']['adam_step']==initial['adam_step']+count,
            'final full state differs from native completion')
    require(completion['source']['checkpoint']==checkpoints[-1],'final checkpoint binding differs')
    return {'source_build':build,'config':canonical,'exact_training_contract_sha256':training_contract(canonical),
            'implementation_sha256':start['implementation_sha256'],'feature_contract_digest':start['feature_contract_digest'],
            'feature_encoding_digest':start['feature_encoding_digest'],
            'initial_full_state_sha256':initial_digest,'initial_state_sha256':initial['state_sha256'],
            'final_state_sha256':state,'iteration_state_sha256s':states,'full_state_payload_sha256s':state_hashes,
            'first_batch_semantic_sha256s':first,'first_batch_raw_sha256s':first_raw,
            'all_trajectory_semantics':ALL_TRAJECTORY_SCHEMA,'all_trajectory_semantic_sha256s':all_semantic,
            'all_trajectory_raw_pins':all_pins,'normalized_trajectory_fields':normalizations,
            'checkpoints':checkpoints,'collections':collections,'completed_updates':count,'completed_fresh_games':count*10,
            'preparation_workers':preparation_workers(config),'update_preparation_executions':preparation,
            'raw_completion':spec['completion']}


def training_parity(reader,left_spec,right_spec):
    left=training_run(reader,left_spec);right=training_run(reader,right_spec)
    for key in ('exact_training_contract_sha256','implementation_sha256','feature_contract_digest',
                'feature_encoding_digest','initial_full_state_sha256','initial_state_sha256',
                'final_state_sha256','iteration_state_sha256s','full_state_payload_sha256s',
                'first_batch_semantic_sha256s','all_trajectory_semantics','all_trajectory_semantic_sha256s',
                'completed_updates','completed_fresh_games','preparation_workers'):
        require(left[key]==right[key],'actual native training parity differs: '+key)
    require(left['source_build']['source_commit']==right['source_build']['source_commit'],
            'training parity source commits differ')
    return left,right


def resume_parity(reader,spec):
    left,right=training_parity(reader,spec['uninterrupted'],spec['resumed'])
    launches=spec['resumed']['launch_results']
    require(len(launches)>=2,'resume requires actual separate invocation evidence')
    prior=0;interrupted=False;failed_launches=[]
    for index,reference in enumerate(launches):
        row=reader.checked(reference)
        validate_preparation_result(right['config'],row)
        require(row['config']['sha256']==spec['resumed']['config']['sha256'] and
                row['binary']['sha256']==right['source_build']['binaries']['native_expanded_training_run_v1'],
                'resume invocation config/binary differs')
        require(row.get('completed_iterations_before')==prior,'resume invocation prefix is unrecorded or discontinuous')
        if row['exit_code']==0 and row['reason']=='engine_exit':
            new=row.get('new_iterations')
            require(type(new) is int and new>0,'resume invocation added no measured work')
            prior+=new
        else:
            require(index<len(launches)-1 and row['exit_code']!=0,'invalid interrupted invocation')
            require(row.get('new_iterations') in (None,0),'failed invocation advanced an unverified prefix')
            interrupted=True;failed_launches.append(reference)
    require(prior==right['completed_updates'],'resume invocations do not cover the complete run')
    snapshot=spec.get('interruption')
    require(interrupted and snapshot,'actual interrupted recovery evidence required, not only clean-boundary reopening')
    interrupted_record=reader.checked(snapshot)
    require(interrupted_record.get('partial_files'),'interruption snapshot has no preserved partial work')
    launch=interrupted_record.get('launch')
    require(launch and any(launch['sha256']==item['sha256'] and normal(launch['path'])==normal(item['path'])
                          for item in failed_launches),
            'interruption snapshot does not bind the actual failed invocation')
    reader.local_pin(launch)
    run_root=normal(reader.checked(spec['resumed']['config'])['output_directory'])
    phases=set()
    for reference in interrupted_record['partial_files']:
        reader.local_pin(reference)
        match=re.fullmatch(re.escape(run_root)+r'/iterations/([0-9]{6})/(attempt-[0-9]{6})/(collect|update)/(.+)',normal(reference['path']))
        require(match is not None,'partial evidence is outside the interrupted native attempt')
        index=int(match[1]);attempt=run_root+f'/iterations/{index:06d}/'+match[2];phase=match[3]
        require(index<len(right['collections']) and not normal(right['collections'][index]['path']).startswith(attempt+'/'),
                'resume reused the interrupted attempt as a completed iteration')
        receipt=attempt+'/'+phase+('/collection.json' if phase=='collect' else '/update.json')
        require(not reader.resolve(receipt).exists(),'interruption evidence points to an already completed native phase')
        phases.add(phase)
    return {'schema':'phase1-actual-resume-parity/v1','passed':True,
            'source_build':right['source_build'],'exact_training_contract_sha256':right['exact_training_contract_sha256'],
            'full_state_payload_sha256s':right['full_state_payload_sha256s'],
            'all_trajectory_semantics':right['all_trajectory_semantics'],
            'all_trajectory_semantic_sha256s':right['all_trajectory_semantic_sha256s'],
            'uninterrupted_trajectory_raw_pins':left['all_trajectory_raw_pins'],
            'resumed_trajectory_raw_pins':right['all_trajectory_raw_pins'],
            'uninterrupted_normalized_fields':left['normalized_trajectory_fields'],
            'resumed_normalized_fields':right['normalized_trajectory_fields'],
            'initial_state_sha256':right['initial_state_sha256'],'final_state_sha256':right['final_state_sha256'],
            'collection_workers':right['config'].get('collection_workers',1),
            'preparation_workers':right['preparation_workers'],
            'update_preparation_executions':right['update_preparation_executions'],
            'uninterrupted_completion':left['raw_completion'],'resumed_completion':right['raw_completion'],
            'interruption':snapshot,'verified_partial_files':len(interrupted_record['partial_files']),
            'observed_interruption_phases':sorted(phases),
            'engineering_only':True,
            'limits':['Equality covers every checkpoint parameter and Adam state plus every complete trajectory field under the explicit provenance normalization',
                      'The supplied interruption location is the tested failure case; other phases and provider/host loss are not implied']}


def bo3_run(reader,spec):
    config=reader.checked(spec['config']);root=config['output_directory'].rstrip('/\\')
    require(set(config)=={'mode','model_sources','output_directory','policies','matches'} and
            config['mode']=='run_population_batch' and config['matches'],'population BO3 config required')
    require(config['policies']==[{'kind':'keep'},{'kind':'keep'}],
            'initial qualification supports exact frozen Keep/Keep BO3 only')
    require(len(config['model_sources'])==2,'BO3 requires two source models')
    build=source_build(reader,spec['source_build'])
    start=reader.expected(spec['run_start'],root+'/run-start.json')
    completion=reader.expected(spec['completion'],root+'/completion.json')
    require(start['schema']=='kernel-population-sideboard-cli-start/v1' and start['command']==config,
            'BO3 start/config differs')
    require(start['build']['git_commit']==build['source_commit'] and
            start['binary']['sha256']==build['binaries']['learned_sideboard_v1'],
            'BO3 executable/source differs')
    reader.local_pin(start['binary'])
    require(start['execution']=={'device':'cpu','gpu_ordinal':None},'BO3 CPU execution required')
    require(len(start['play_models'])==2 and len(start['sideboard_policy_identities'])==2,
            'BO3 requires both model and sideboard policy identities')
    require(completion['mode']=='run_population_batch' and
            completion['completed_matches']==len(config['matches'])==len(spec['matches']),
            'BO3 batch is incomplete')
    require(completion['play_models']==start['play_models'] and
            completion['sideboard_policy_identities']==start['sideboard_policy_identities'],
            'BO3 model/policy completion identities differ')
    for reference in start['inputs']:reader.local_pin(reference)
    require(start['inputs']==completion['inputs'] and start['inputs'][0]['sha256']==spec['config']['sha256'],
            'BO3 input chain differs')
    for source,identity in zip(config['model_sources'],start['play_models']):
        reader.local_pin(source['play_import'])
        if source.get('checkpoint'):
            state,_=checkpoint_state(reader,source['checkpoint'])
            require(identity['checkpoint_sha256']==source['checkpoint']['sha256'] and
                    identity['state_sha256']==state['state_sha256'],'BO3 checkpoint/model binding differs')
    artifacts=[];games=0
    for index,reference in enumerate(spec['matches']):
        value=reader.expected(reference,root+f'/match-{index:06d}.json')
        require(value['schema']=='kernel_population_bo3/v1' and value['config']==config['matches'][index]['config'],
                'BO3 full-match configuration differs')
        require(value['play_models']==start['play_models'] and
                value['sideboard_policy_identities']==start['sideboard_policy_identities'],
                'BO3 match policy/model binding differs')
        require(len(value['explicit_registrations'])==2 and all(
            all(actual[key]==registered[key] for key in ('label','mainboard','sideboard'))
            for actual,registered in zip(value['explicit_registrations'],config['matches'][index]['registered'])),
            'BO3 exact registrations differ')
        require(2<=len(value['games'])<=value['config']['max_physical_games'] and
                set(value['outcome'])=={'winner'},'BO3 match did not reach a complete terminal match result')
        outcome=value['outcome']['winner']
        require(set(outcome)=={'winner'} and type(outcome['winner']) is int and outcome['winner'] in (0,1),
                'BO3 terminal winner is malformed')
        wins=[0,0]
        for game_index,game in enumerate(value['games'],1):
            require(set(game)=={'start','environment_seed','mainboard_sha256','winner'} and
                    set(game['start'])=={'game_index','chooser','choice','starting_player'} and
                    game['start']['game_index']==game_index and len(game['mainboard_sha256'])==2,
                    'BO3 game record is incomplete')
            require(max(wins)<2,'BO3 continued after the match was already complete')
            winner=game['winner']
            require(winner is None or (type(winner) is int and winner in (0,1)),'invalid physical game winner')
            if winner is not None:wins[winner]+=1
        require(wins[outcome['winner']]==2,'BO3 terminal match outcome is inconsistent with physical games')
        require(len(value['sideboard_decisions'])==2*(len(value['games'])-1),
                'BO3 artifact lacks complete between-game sideboarding records')
        for row in value['sideboard_decisions']:
            require(set(row)=={'acting_player','input','initial_mainboard','initial_sideboard',
                               'selected_actions','selected_mainboard_sha256'},'BO3 sideboard record is incomplete')
        # Entire JSON is compared. No outcome, decision, registration, model or
        # arbitrary path/hash field is removed from a complete match artifact.
        artifacts.append(digest(value));games+=len(value['games'])
    require(games==completion['physical_games'],'BO3 physical game count differs')
    canonical=copy.deepcopy(config);canonical.pop('output_directory')
    # Exact canonical model sources for cross-platform import relocation only.
    dummy={'initial_source':config['model_sources'][0],
           'opponents':[{'id':'seat1','source':config['model_sources'][1]}]}
    original,_=original_config(reader,dummy,spec.get('payload'))
    canonical['model_sources']=[original['initial_source'],original['opponents'][0]['source']]
    return {'source_build':build,'config':canonical,'config_semantic_sha256':digest(canonical),
            'compiled_sources':start['compiled_sources'],'tag_file_sha256':start['tag_file_sha256'],
            'full_match_semantic_sha256s':artifacts,'physical_games':games,'completed_matches':len(artifacts)}


def bo3_parity(reader,spec):
    left=bo3_run(reader,spec['left']);right=bo3_run(reader,spec['right'])
    for key in ('config_semantic_sha256','compiled_sources','tag_file_sha256',
                'full_match_semantic_sha256s','physical_games','completed_matches'):
        require(left[key]==right[key],'actual BO3 parity differs: '+key)
    require(left['source_build']['source_commit']==right['source_build']['source_commit'],
            'BO3 parity source commits differ')
    return {'schema':'phase1-actual-bo3-parity/v1','passed':True,'source_build':right['source_build'],
            'config':right['config'],'config_semantic_sha256':right['config_semantic_sha256'],
            'full_match_semantic_sha256s':right['full_match_semantic_sha256s'],
            'completed_matches':right['completed_matches'],'physical_games':right['physical_games'],
            'engineering_only':True,
            'limits':['Full recorded BO3 match/game/sideboarding equality, not a playing-strength result',
                      'Native BO3 artifacts do not contain a complete gameplay action trace',
                      'This version qualifies frozen Keep/Keep only; learned sideboard heads require separate evidence']}
