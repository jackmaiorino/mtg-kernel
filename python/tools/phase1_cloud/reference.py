"""Read native completed-run artifacts into a fixed-work throughput reference.

No engine execution and no win/loss or loss-based selection. Both local and cloud
references are derived from native receipts and the actual bounded supervisor.
"""
from __future__ import annotations
import argparse
import hashlib
from pathlib import Path
from common import encoded, pin, read, require, write
from throughput import (finite, training_contract, canonical_training_config, preparation_workers,
                        validate_preparation_runner, validate_preparation_update, validate_preparation_result)
from semantic import ALGORITHM, trajectory_digest, validated_map
from fresh_source import fresh_origins, initial_state


def checked(reference):
    pin(reference['path'],reference['sha256'])
    return read(reference['path'])


def build(config_pin, result_pin, build_pin, payload_pin=None, inspection_pin=None):
    config=checked(config_pin); wrapper=checked(result_pin); build=checked(build_pin)
    validate_preparation_result(config,wrapper)
    require(wrapper.get('schema') in ('phase1-auxiliary-cpu-result/v1','phase1-cloud-worker-result/v1'),
            'unsupported native supervisor result schema')
    require(wrapper['exit_code']==0 and wrapper['reason']=='engine_exit','native wrapper did not complete')
    require(wrapper['config']['sha256']==config_pin['sha256'],'wrapper config differs')
    expected_binary=build.get('binary',build.get('binaries',{}).get('native_expanded_training_run_v1'))
    require(expected_binary and wrapper['binary']['sha256']==expected_binary['sha256'],
            'wrapper binary differs from source build')
    root=Path(config['output_directory'])
    start=read(root/'run.json')
    require(start['config']==canonical_training_config(config),'native start config differs')
    validate_preparation_runner(config,start)
    completion=read(root/'completion.json')
    validate_preparation_runner(config,completion)
    count=len(config['iterations'])
    require(completion['complete'] is True and completion['completed_iterations']==count and
            len(completion['iterations'])==count and count>0,'fixed work incomplete')
    require(wrapper.get('completed_iterations_before')==0 and wrapper.get('new_iterations')==count,
            'reference requires one fresh whole invocation; missing delta, no-op or partial resume rejected')
    relocations=validated_map(payload_pin,config) if payload_pin else None
    source=config['initial_source']
    if source.get('checkpoint') is None:
        # A fresh Adam0 learner has no checkpoint; its initial state is the
        # native inspection proof, never a fabricated resumable checkpoint.
        if inspection_pin is None and payload_pin:
            inspection_pin=checked(payload_pin).get('initial_inspection')
        require(inspection_pin is not None,'fresh Adam0 initial source requires actual native inspection evidence')
        equivalents=[row['original'] for row in (relocations or []) if row['relocated']==source]
        initial=initial_state(source,inspection_pin,equivalent_sources=equivalents)
    else:
        initial=checked(source['checkpoint'])
    origins=fresh_origins(config)
    states=[]; first_hashes=[]; first_semantic=[]; first_pins=[]; stage_seconds=[]; instrumentation_complete=True
    preparation=[]
    for index,complete_pin in enumerate(completion['iterations']):
        complete=checked(complete_pin)
        require(complete['iteration']==index,'iteration order differs')
        collection=checked(complete['collection']); update=checked(complete['update'])
        execution=validate_preparation_update(config,update)
        if execution is not None:preparation.append({'iteration':index,'update':complete['update'],**execution})
        require(collection['complete'] is True and update['complete'] is True,'incomplete native phase')
        require(update['before_state_sha256']==(initial['state_sha256'] if index==0 else states[-1]),
                'update chain differs')
        require(update['after_state_sha256']==complete['output_identity']['state_sha256'],
                'update output state differs')
        saved=checked(update['checkpoint'])
        require(saved['state_sha256']==update['after_state_sha256'] and
                saved['adam_step']==initial['adam_step']+index+1,'full-state checkpoint or Adam step differs')
        require(len(collection['trajectories'])==len(config['iterations'][index]['episodes']),
                'completed episode count differs')
        hashes=[]
        for trajectory in collection['trajectories']:
            pin(trajectory['path'],trajectory['sha256']);hashes.append(trajectory['sha256'])
        if index==0:
            first_hashes=hashes
            first_pins=collection['trajectories']
            first_semantic=[trajectory_digest(checked(item),config,relocations,origins) for item in first_pins]
        states.append(update['after_state_sha256'])
        c=collection.get('collection_elapsed_seconds');u=update.get('update_elapsed_seconds')
        if finite(c) and finite(u): stage_seconds.append(c+u)
        else: instrumentation_complete=False
    require(states[-1]==completion['actual_identity']['state_sha256'],'final native state differs')
    elapsed=wrapper.get('native_execution_elapsed_seconds',wrapper.get('native_active_seconds',wrapper.get('elapsed_seconds')))
    require(finite(elapsed,True),'wrapper elapsed time unavailable')
    require(sum(stage_seconds)<=elapsed+1,'native stage timings exceed wrapper envelope')
    max_iteration=(max(stage_seconds)+max(0,elapsed-sum(stage_seconds))) if instrumentation_complete else elapsed
    contract=training_contract(config)
    if payload_pin:
        payload=checked(payload_pin)
        require(payload['training_config']['sha256']==config_pin['sha256'] and
                payload['source_commit']==build['source_commit'],'payload source/config differs')
        contract=payload['training_contract_sha256']
    canonical=canonical_training_config(config)
    work={'iterations':config['iterations'],'learning_rate':canonical['learning_rate'],
          'value_coefficient':canonical['value_coefficient'],'initial_state_sha256':initial['state_sha256']}
    return {'schema':'phase1-fixed-work-training/v1','complete':True,
        'source_commit':build['source_commit'],'training_contract_sha256':contract,
        'fixed_work_sha256':hashlib.sha256(encoded(work)).hexdigest(),
        'initial_state_sha256':initial['state_sha256'],'final_state_sha256':states[-1],
        'iteration_state_sha256s':states,'first_batch_episode_sha256s':first_hashes,
        'first_batch_trajectory_pins':first_pins,'trajectory_semantics':ALGORITHM,
        'first_batch_semantic_sha256s':first_semantic,
        'completed_games':sum(len(item['episodes']) for item in config['iterations']),
        'completed_iterations':count,'collection_workers':config.get('collection_workers',1),
        'preparation_workers':preparation_workers(config),'update_preparation_executions':preparation,
        'elapsed_seconds':elapsed,'max_iteration_elapsed_seconds':max_iteration,
        'measurement_scope':'native_supervisor_invocation_only',
        'complete_work_timing_available':False,
        'max_iteration_is_conservative_envelope':True,
        'stage_instrumentation_complete':instrumentation_complete,
        'config':config_pin,'supervisor_result':result_pin,'source_build':build_pin,
        'native_completion':pin(root/'completion.json'),'payload':payload_pin,
        'native_run_start':pin(root/'run.json'),
        'initial_inspection':{key:inspection_pin[key] for key in ('path','sha256')} if inspection_pin else None,
        'engineering_only':True,'strength_claim':False}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',required=True);parser.add_argument('--result',required=True)
    parser.add_argument('--build',required=True);parser.add_argument('--payload')
    parser.add_argument('--initial-inspection',help='native fresh Adam0 inspection for a checkpoint-less initial source')
    parser.add_argument('--output',required=True)
    args=parser.parse_args()
    result=build(pin(args.config),pin(args.result),pin(args.build),pin(args.payload) if args.payload else None,
                 pin(args.initial_inspection) if args.initial_inspection else None)
    write(args.output,result)
    print(encoded({'output':pin(args.output),'completed_games':result['completed_games'],
        'elapsed_seconds':result['elapsed_seconds']}).decode())


if __name__=='__main__': main()
