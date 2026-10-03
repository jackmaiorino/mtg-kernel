"""Offline computational checks for one pinned, bounded future training run.

This is a small run-manifest consumer, not an authorization protocol or launcher.
Live production remains unavailable in cloud_worker.py pending root review and
actual qualification. Nothing in this module allocates or executes work.
"""
from __future__ import annotations
import argparse
import copy
from common import encoded, pin, read, require, write
from parity_evidence import Artifacts, bo3_parity, resume_parity, source_build, training_parity, original_config, digest
from throughput import qualify, preparation_workers, validate_preparation_result
from workload import compare
from runtime_compatibility import compare as compare_runtime


def model_key(source):
    value=copy.deepcopy(source)
    for key in ('play_import','checkpoint'):
        if value.get(key):value[key]={'sha256':value[key]['sha256']}
    return digest(value)


def reference_run(reader,reference):
    require(reference.get('native_run_start'),'reference lacks a pinned native run-start artifact')
    spec={'config':reference['config'],'source_build':reference['source_build'],
          'run_start':reference['native_run_start'],'completion':reference['native_completion']}
    if reference.get('payload'):spec['payload']=reference['payload']
    if reference.get('initial_inspection'):spec['initial_inspection']=reference['initial_inspection']
    wrapper=reader.checked(reference['supervisor_result'])
    validate_preparation_result(reader.checked(spec['config']),wrapper)
    require(wrapper.get('schema') in ('phase1-auxiliary-cpu-result/v1','phase1-cloud-worker-result/v1') and
            wrapper['exit_code']==0 and wrapper['reason']=='engine_exit',
            'qualification has no successful actual native invocation')
    require(wrapper['config']['sha256']==spec['config']['sha256'] and
            wrapper.get('completed_iterations_before')==0 and
            wrapper.get('new_iterations')==reference['completed_iterations'],
            'qualification invocation is not one fresh complete run')
    elapsed=wrapper.get('native_execution_elapsed_seconds',wrapper.get('native_active_seconds',wrapper.get('elapsed_seconds')))
    require(elapsed==reference['elapsed_seconds'],'reference elapsed differs from actual invocation')
    build=source_build(reader,spec['source_build'])
    require(wrapper['binary']['sha256']==build['binaries']['native_expanded_training_run_v1'],
            'qualification invocation used a different executable')
    return spec


def bind_reference(reference,actual):
    fields={'training_contract_sha256':'exact_training_contract_sha256',
            'initial_state_sha256':'initial_state_sha256','final_state_sha256':'final_state_sha256',
            'iteration_state_sha256s':'iteration_state_sha256s',
            'first_batch_semantic_sha256s':'first_batch_semantic_sha256s',
            'first_batch_episode_sha256s':'first_batch_raw_sha256s',
            'completed_iterations':'completed_updates','completed_games':'completed_fresh_games'}
    for reported,measured in fields.items():
        require(reference[reported]==actual[measured],'qualification reference differs from actual artifacts: '+reported)
    require(reference['source_commit']==actual['source_build']['source_commit'],
            'qualification reference source differs')
    require(reference['collection_workers']==actual['config'].get('collection_workers',1),
            'qualification reference workers differ')
    require(preparation_workers(reference)==actual['preparation_workers'],
            'qualification reference preparation workers differ')


def check(manifest_pin):
    manifest=read(pin(manifest_pin['path'],manifest_pin['sha256'])['path'])
    required={'schema','qualification','production_config','production_source_build','max_updates',
              'bo3_parity','resume_parity','runtime_parity'}
    require(required<=set(manifest)<=required|{'path_mappings','production_payload'},
            'unsupported future run-manifest fields')
    require(manifest['schema']=='phase1-production-engineering-run/v1','wrong future run-manifest schema')
    reader=Artifacts(manifest.get('path_mappings',[]))
    proof=reader.checked(manifest['qualification'])
    actual_proof=qualify(proof['local_reference'],proof['cloud_reference'],
                        proof.get('local_timing'),proof.get('cloud_timing'),artifacts=reader)
    require(proof|{'preparation_workers':preparation_workers(proof)}==actual_proof and actual_proof['passed'],
            'actual complete-work qualification has not established required speedup')
    local_ref=reader.checked(proof['local_reference']);cloud_ref=reader.checked(proof['cloud_reference'])
    local_spec=reference_run(reader,local_ref);cloud_spec=reference_run(reader,cloud_ref)
    local,cloud=training_parity(reader,local_spec,cloud_spec)
    bind_reference(local_ref,local);bind_reference(cloud_ref,cloud)
    future_raw=reader.checked(manifest['production_config'])
    future,_=original_config(reader,future_raw,manifest.get('production_payload'))
    future_build=source_build(reader,manifest['production_source_build'])
    require(future_build==cloud['source_build'],'future production build/runtime inputs differ from qualified Linux build')
    compatibility=compare(cloud['config'],future,manifest['max_updates'])
    bo3=bo3_parity(reader,manifest['bo3_parity'])
    recovery=resume_parity(reader,manifest['resume_parity'])
    recovery_launches=manifest['resume_parity']['resumed']['launch_results']
    recovery_schemas={reader.checked(item).get('schema') for item in recovery_launches}
    require(recovery_schemas in ({'phase1-auxiliary-cpu-result/v1'},{'phase1-cloud-worker-result/v1'}),
            'recovery invocations have mixed or unsupported execution environments')
    recovery_is_cloud=recovery_schemas=={'phase1-cloud-worker-result/v1'}
    recovery_runs=[manifest['resume_parity']['uninterrupted']['run_start'],
                   manifest['resume_parity']['resumed']['run_start']]
    runtime=compare_runtime(reader,manifest['runtime_parity'],local['source_build'],cloud['source_build'],
        [local_ref['supervisor_result'],*([] if recovery_is_cloud else recovery_launches)],
        [cloud_ref['supervisor_result'],*(recovery_launches if recovery_is_cloud else [])],
        [local_spec['run_start'],manifest['bo3_parity']['left']['run_start'],
         *([] if recovery_is_cloud else recovery_runs)],
        [cloud_spec['run_start'],manifest['bo3_parity']['right']['run_start'],
         *(recovery_runs if recovery_is_cloud else [])])
    require(bo3['source_build']==future_build and recovery['source_build']==future_build,
            'BO3/recovery artifacts do not qualify this exact production build')
    require(recovery['exact_training_contract_sha256']==cloud['exact_training_contract_sha256'] and
            recovery['collection_workers']==cloud['config'].get('collection_workers',1) and
            recovery['preparation_workers']==cloud['preparation_workers'] and
            recovery['initial_state_sha256']==cloud['initial_state_sha256'] and
            recovery['full_state_payload_sha256s']==cloud['full_state_payload_sha256s'] and
            recovery['all_trajectory_semantic_sha256s']==cloud['all_trajectory_semantic_sha256s'],
            'recovery fixture differs from the qualified fixed work or worker configuration')
    allowed_models={model_key(source) for source in [cloud['config']['initial_source'],
                    *[opponent['source'] for opponent in cloud['config']['opponents']]]}
    tested_models=[model_key(source) for source in bo3['config']['model_sources']]
    require(set(tested_models)<=allowed_models and model_key(cloud['config']['initial_source']) in tested_models,
            'BO3 parity did not exercise the qualified initial model and permitted opponent sources')
    decks={digest(deck) for iteration in cloud['config']['iterations'] for item in iteration['episodes']
           for deck in item['episode']['registered']}
    for match in bo3['config']['matches']:
        require(all(digest(deck) in decks for deck in match['registered']),
                'BO3 parity uses registrations outside the qualified workload')
    return {'schema':'phase1-production-engineering-check/v1','run_manifest':manifest_pin,
            'computational_prerequisites_satisfied':True,'live_production_enabled':False,
            'production_eligible':False,'production_config':manifest['production_config'],
            'workload_compatibility':compatibility,'complete_work_qualification':manifest['qualification'],
            'actual_bo3_parity':bo3,'actual_resume_parity':recovery,'actual_runtime_compatibility':runtime,
            'actual_training_parity':{'schema':cloud['all_trajectory_semantics'],
                'all_trajectory_semantic_sha256s':cloud['all_trajectory_semantic_sha256s'],
                'local_trajectory_raw_pins':local['all_trajectory_raw_pins'],
                'cloud_trajectory_raw_pins':cloud['all_trajectory_raw_pins'],
                'local_normalized_fields':local['normalized_trajectory_fields'],
                'cloud_normalized_fields':cloud['normalized_trajectory_fields'],
                'full_state_payload_sha256s':cloud['full_state_payload_sha256s']},
            'verified_artifact_count':len(reader.used),'engineering_only':True,'strength_claim':False,
            'remaining_limits':['Live production remains disabled until root review and actual cloud/runtime qualification',
                'Workload compatibility does not forecast constant throughput as learned behavior changes',
                'BO3 parity covers recorded artifacts with frozen Keep/Keep, not learned sideboard strength',
                'The tested interruption phase does not establish every failure mode or Pod/host-loss recovery',
                'CPU entitlement can include unobservable ancestor limits; occupancy is not inferred from affinity alone']}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest',required=True);parser.add_argument('--output',required=True)
    args=parser.parse_args()
    result=check(pin(args.manifest));write(args.output,result)
    print(encoded({'output':pin(args.output),'computational_prerequisites_satisfied':True,
                   'live_production_enabled':False}).decode())


if __name__=='__main__':main()
