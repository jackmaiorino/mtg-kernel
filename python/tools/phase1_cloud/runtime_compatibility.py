"""Read pinned Linux userspace observations; never infer cross-libm parity.

Runtime observations accompany the actual invocation and native-start receipts.
The numerical comparison remains exact and independent of this environment
check. Kernel and CPU differences are recorded, never normalized away.
"""
from __future__ import annotations
import re
from common import require
from runtime_observation import IMAGE_REFERENCE, LIBRARIES, image_contract, verify_environment
from throughput import canonical_training_config, validate_preparation_result, validate_preparation_runner


def runtime(reader, reference, build, invocations, native_runs, environment):
    value=reader.checked(reference)
    required={'schema','environment','runtime_image_pins','invocations','pod_id'}
    require(required<=set(value)<=required|{'host_context'},'incomplete or unsupported sampled runtime bundle')
    require(value['schema']=='phase1-native-runtime/v2' and value['environment']==environment,
            'actual sampled native-runtime v2 bundle required; aggregate labels are insufficient')
    image=reader.checked(value['runtime_image_pins']);expected=image_contract(image)
    host=reader.checked(value['host_context']) if value.get('host_context') else {}
    if host:
        require(host.get('schema')=='phase1-linux-userspace-import/v1','unsupported local host context')
        reader.local_pin(host['image_pins'])
        require(host['image_pins']['sha256']==value['runtime_image_pins']['sha256'],'host context belongs to another image')
    rows=value['invocations'];require(isinstance(rows,list) and rows,'actual invocation pins required')
    observed={(item['path'],item['sha256']) for item in rows}
    require(all((item['path'],item['sha256']) in observed for item in invocations),
            'runtime bundle does not cover an actual required invocation')
    require((environment=='cloud' and isinstance(value['pod_id'],str) and value['pod_id']) or
            (environment=='local' and value['pod_id'] is None),'runtime host identity unavailable or inconsistent')
    observations=[];covered=set();executables={};cpu_models=set();feature_sets=set();kernels=set()
    for invocation_pin in rows:
        invocation=reader.checked(invocation_pin)
        required_schema='phase1-cloud-worker-result/v1' if environment=='cloud' else 'phase1-auxiliary-cpu-result/v1'
        require(invocation.get('schema')==required_schema,'runtime invocation environment differs')
        if environment=='cloud':require(invocation.get('pod_id')==value['pod_id'],'runtime Pod invocation differs')
        image_pin=invocation.get('runtime_image_pins');sample_pin=invocation.get('runtime_observation')
        require(image_pin and sample_pin,'actual sampled runtime or image binding unavailable')
        reader.local_pin(image_pin)
        require(image_pin['sha256']==value['runtime_image_pins']['sha256'],'invocation runtime image differs')
        sample=reader.checked(sample_pin)
        require(sample.get('schema') in ('phase1-sampled-linux-runtime/v1','phase1-sampled-linux-runtime/v2') and
                sample.get('image_reference')==IMAGE_REFERENCE,'unsupported or different sampled userspace')
        verify_environment(sample['environment'])
        require(type(sample.get('pid')) is int and sample['pid']>0 and
                type(sample.get('proc_starttime_ticks')) is int and sample['proc_starttime_ticks']>=0 and
                isinstance(sample.get('maps'),str) and sample['maps'],'owned-process mapping observation unavailable')
        if invocation.get('pid') is not None:require(invocation['pid']==sample['pid'],'sample belongs to another child')
        def verify_file(item):
            captured=sample.get('captured_files',{}).get(item['path'])
            if captured:
                require(captured['sha256']==item['sha256'] and captured['bytes']==item['bytes'],
                        'captured runtime bytes differ from mapped observation')
                reader.local_pin(captured)
            else:reader.local_pin(item)
        binary=sample['executable'];verify_file(binary)
        require(binary['sha256']==invocation['binary']['sha256'],'sample executable differs from actual invocation')
        roles=[role for role,sha in build['binaries'].items() if sha==binary['sha256']]
        require(len(roles)==1,'sample executable is not an unambiguous qualified trainer/evaluator')
        role=roles[0];executables[role]=binary['sha256']
        if role=='native_expanded_training_run_v1':
            validate_preparation_result(reader.checked(invocation['config']),invocation)
        require(set(sample['libraries'])==set(LIBRARIES),'sample omitted a required mapped library')
        map_paths={line.split(maxsplit=5)[5] for line in sample['maps'].splitlines() if len(line.split(maxsplit=5))==6}
        require(binary['path'] in map_paths,'sample raw maps omit the executing binary')
        for name,item in sample['libraries'].items():
            require(item['path']==expected[name]['resolved_path'] and item['sha256']==expected[name]['sha256'] and
                    item['bytes']==expected[name]['bytes'] and item['path'] in map_paths,
                    'sample mapped library differs from image or raw maps')
            verify_file(item)
        kernel=sample.get('kernel_release');require(isinstance(kernel,str) and kernel,'sampled kernel unavailable')
        if host:require(host['kernel_release']==kernel,'local host context kernel differs from sample')
        cpu=sample.get('cpu_model',host.get('cpu_model'));features=sample.get('cpu_features',host.get('cpu_features'))
        quota=sample.get('quota_context')
        if sample['schema']=='phase1-sampled-linux-runtime/v2':
            require(isinstance(cpu,str) and cpu and isinstance(features,list) and features and
                    isinstance(quota,dict) and quota.get('affinity') and isinstance(quota.get('quota'),dict),
                    'live sampled CPU/quota context unavailable')
        if cpu:cpu_models.add(cpu)
        if features:
            require(all(isinstance(item,str) and item for item in features) and len(set(features))==len(features),
                    'CPU feature observation malformed')
            feature_sets.add(tuple(sorted(features)))
        kernels.add(kernel)
        start=invocation.get('native_run_start')
        if start:
            native_start=reader.checked(start);configuration=reader.checked(invocation['config'])
            if role=='native_expanded_training_run_v1':validate_preparation_runner(configuration,native_start)
            require((role=='learned_sideboard_v1' and native_start.get('command')==configuration) or
                    (role=='native_expanded_training_run_v1' and
                     native_start.get('config')==canonical_training_config(configuration)),
                    'sampled invocation config differs from the claimed native start')
            if invocation['exit_code']==0 and invocation['reason']=='engine_exit':covered.add((start['path'],start['sha256']))
        observations.append({'invocation':invocation_pin,'sample':sample_pin,'role':role,'pid':sample['pid'],
            'cpu_model':cpu,'cpu_features':features,'kernel_release':kernel,'quota_context':quota,
            'cpu_context_source':'sample' if sample.get('cpu_model') else 'pinned pre-run host context' if host else 'unavailable',
            'quota_context_source':'actual sample' if quota is not None else 'unavailable',
            'one_time_sample':True})
    require(all((item['path'],item['sha256']) in covered for item in native_runs),
            'no successful sampled invocation covers a required native trainer/BO3/recovery start')
    require(set(executables)=={'native_expanded_training_run_v1','learned_sideboard_v1'},
            'actual trainer and evaluator mappings are both required')
    require(len(cpu_models)<=1 and len(feature_sets)<=1 and len(kernels)==1,'runtime host observations disagree')
    return {'record':reference,'userspace':{'image_digest':IMAGE_REFERENCE.split('@')[1],
            'platform':'linux/amd64','executables':executables,
            'libraries':{name:row['sha256'] for name,row in expected.items()}},
            'cpu_model':next(iter(cpu_models),None),'cpu_features':list(next(iter(feature_sets),())),
            'kernel_release':next(iter(kernels)),'observations':observations,'pod_id':value['pod_id'],
            'verified_native_runs':len(set((p['path'],p['sha256']) for p in native_runs)),
            'verified_invocations':len(observed)}


def compare(reader, spec, local_build, cloud_build, local_invocations, cloud_invocations,
            local_runs, cloud_runs):
    require(set(spec)=={'local','cloud'},'both local and cloud runtime records required')
    left=runtime(reader,spec['local'],local_build,local_invocations,local_runs,'local')
    right=runtime(reader,spec['cloud'],cloud_build,cloud_invocations,cloud_runs,'cloud')
    require(left['userspace']==right['userspace'],
            'Linux image/executable/loader/libc/libm/libgcc differs between qualified executions')
    return {'schema':'phase1-native-runtime-comparison/v2','userspace_identical':True,
            'qualified_userspace':right['userspace'],'local':left,'cloud':right,
            'hardware_observations_identical':bool(left['cpu_model'] and right['cpu_model']) and all(left[key]==right[key] for key in
                ('cpu_model','cpu_features','kernel_release')),
            'limits':['Image and library identity is a prerequisite, not a substitute for exact cross-host numerical parity',
                      'CPU features and kernel differences are retained; matching recorded userspace does not prove all future hosts equivalent',
                      'Each mapping observation is a one-time sample; missing legacy CPU/quota context stays unavailable',
                      'A future executing host must supply matching runtime observations before production can be enabled']}
