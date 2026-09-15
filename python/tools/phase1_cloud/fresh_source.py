"""Pure fresh-origin transport checks. Never initialize a model or reset Adam.

Only descriptor/manifest/payload files are opened. Producer source, library and
registry paths in the immutable manifest are historical provenance, not inputs.
Native loaders remain the executable admission authority.
"""
from __future__ import annotations
import array
import hashlib
import json
import math
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import struct
import sys
from common import pin, require, unique_pairs

MIB = 1024 * 1024
SOURCE_SCHEMA = 'mtg-kernel-fresh-initialization-source/v1'
MANIFEST_SCHEMA = 'mtg-kernel-fresh-initialization/v1'
ORIGIN_SCHEMA = 'mtg-kernel-fresh-play-initialization/v1'
CHECKPOINT_SCHEMA = 'mtg-kernel-expanded-deck-fresh-checkpoint/v1'
TRAJECTORY_SCHEMA = 'mtg-kernel-expanded-deck-trajectory/v3'
PAYLOAD_BYTES = 4923976
SAMPLER = 'f32-q8-expq63-hamilton-splitmix64-wide-v1'
CONFIG_SHA = 'f3836afa17acc74b4856fe18222345116f27c12fa5ad18c34b4dec3f04855251'
SOURCE_FILES = ('python/mtg_kernel_rl/phase1_fresh_initialization_v1.py',
    'python/mtg_kernel_rl/model.py', 'python/mtg_kernel_rl/features.py',
    'python/mtg_kernel_rl/determinism.py', 'python/mtg_kernel_rl/common_model_snapshot_v1.py',
    'python/mtg_kernel_rl/features_v6.py', 'data/flat_policy_v3/feature_contract_v3.json', 'data/cards_v1.json')


def shape(value, fields):
    require(type(value) is dict and set(value) == set(fields), 'fresh document fields differ')


def hex_value(value, length=64):
    require(type(value) is str and re.fullmatch('[0-9a-f]{'+str(length)+'}', value), 'invalid fresh digest')
    return value


def integer(value, maximum=(1 << 64)-1):
    require(type(value) is int and 0 <= value <= maximum, 'invalid fresh integer')
    return value


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True,
                      allow_nan=False).encode('ascii')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def absolute(value):
    require(type(value) is str and value and '\x00' not in value and
            (PurePosixPath(value).is_absolute() or PureWindowsPath(value).is_absolute()),
            'absolute recorded file path required')


def file_pin(value, with_bytes=False):
    shape(value, ('path', 'sha256', 'bytes') if with_bytes else ('path', 'sha256'))
    absolute(value['path']); hex_value(value['sha256'])
    if with_bytes:
        require(0 < integer(value['bytes']) <= 2*1024**3, 'recorded file size invalid')


def load_json(path, cap=MIB):
    path = Path(path)
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= cap,
            'fresh JSON input exceeds bound or is not regular')
    with path.open('rb') as stream:
        data = stream.read(cap+1)
    require(len(data) <= cap, 'fresh JSON input grew beyond bound')
    def reject(value):
        raise ValueError('nonfinite fresh JSON')
    def floating(value):
        result = float(value); require(math.isfinite(result), 'nonfinite fresh JSON'); return result
    return json.loads(data, object_pairs_hook=unique_pairs, parse_constant=reject, parse_float=floating), data


def checked_file(reference, resolve_pin=None, cap=MIB):
    file_pin(reference)
    local = Path(reference['path']) if resolve_pin is None else Path(resolve_pin(reference))
    require(local.is_file() and local.stat().st_size <= cap, 'fresh input exceeds bound')
    actual = pin(local, reference['sha256'])
    return local, actual


def layout():
    shapes = [('card_embedding.weight', [65537, 16])]
    for name, width, outputs in (('object_encoder',114,64), ('edge_encoder',169,64),
            ('node_update',128,64), ('state_encoder',1499,64), ('action_ref_encoder',89,64),
            ('action_encoder',259,64), ('scorer',128,1), ('value_head',64,1)):
        shapes.extend(((name+'.0.weight',[64,width]), (name+'.0.bias',[64]),
                       (name+'.2.weight',[outputs,64]), (name+'.2.bias',[outputs])))
    offset, rows = 0, []
    for ordinal, (name, dimensions) in enumerate(shapes):
        count = math.prod(dimensions)*4
        rows.append(dict(ordinal=ordinal, name=name, shape=dimensions, byte_offset=offset, byte_count=count))
        offset += count
    require(len(rows) == 33 and offset == PAYLOAD_BYTES, 'fresh layout implementation differs')
    return rows


def atom(hasher, tag, payload):
    tag = tag.encode('ascii')
    hasher.update(struct.pack('>I',len(tag))+tag+struct.pack('>Q',len(payload))+payload)


def model_seed(base):
    integer(base, (1 << 63)-1)
    hasher = hashlib.sha256()
    for tag, data in (('version',b'kernel-python-rl-trainer-sha256-v2'), ('namespace',b'model-init'),
                      ('field-name',b'base_seed'), ('u63',struct.pack('>Q',base))):
        atom(hasher,tag,data)
    return int.from_bytes(hasher.digest()[:8],'big') & ((1 << 63)-1)


def is_fresh_origin(value):
    return type(value) is dict and value.get('schema') == ORIGIN_SCHEMA


def validate_origin(origin):
    shape(origin, ('schema','initialization_manifest_sha256','lineage_id','initializer','base_seed',
        'model_init_seed','seed_derivation','producer_git_commit','initial_weights_sha256',
        'initial_model_parameter_sha256','parameter_layout_sha256','destination_registry_sha256',
        'destination_card_db_hash','destination_card_count','feature_contract_digest','feature_encoding_digest',
        'features_source_sha256','feature_descriptor_sha256','sampler_identity'))
    require(origin['schema'] == ORIGIN_SCHEMA and origin['initializer'] == 'trainer-seeded-v1' and
            origin['seed_derivation'] == 'kernel-python-rl-trainer-sha256-v2' and origin['sampler_identity'] == SAMPLER,
            'fresh origin identity differs')
    require(type(origin['lineage_id']) is str and 1 <= len(origin['lineage_id']) <= 128 and
            all(33 <= ord(c) <= 126 for c in origin['lineage_id']), 'invalid fresh lineage ID')
    require(origin['model_init_seed'] == model_seed(origin['base_seed']), 'fresh derived seed differs')
    integer(origin['model_init_seed'], (1 << 63)-1)
    require(0 < integer(origin['destination_card_count']) <= 65536, 'fresh registry count invalid')
    for name in origin:
        if name.endswith('_sha256') or name.endswith('_digest'): hex_value(origin[name])
    hex_value(origin['producer_git_commit'],40); hex_value(origin['destination_card_db_hash'],16)
    return origin


def inspect_source(source_pin, feature_transfer, *, resolve_pin=None):
    dependencies = []
    def document(reference):
        local, actual = checked_file(reference,resolve_pin); dependencies.append(actual)
        value, raw = load_json(local)
        require(sha(raw) == reference['sha256'], 'fresh JSON changed during read')
        return value, raw
    descriptor, _ = document(source_pin)
    shape(descriptor, ('schema','initialization','parameters'))
    require(descriptor['schema'] == SOURCE_SCHEMA, 'fresh source descriptor required')
    manifest, raw_manifest = document(descriptor['initialization'])
    shape(manifest, ('schema','lineage_id','initializer','producer','target','model','payload','parameters','optimizer_bootstrap'))
    require(manifest['schema'] == MANIFEST_SCHEMA and raw_manifest == canonical(manifest)+b'\n',
            'fresh manifest schema/canonical bytes differ')
    init, producer, target = manifest['initializer'], manifest['producer'], manifest['target']
    shape(init, ('identity','authority','base_seed','model_init_seed','seed_derivation'))
    require(init['identity'] == 'trainer-seeded-v1' and
            init['authority'] == 'Python KernelPolicyValueNet.reset_seeded_parameters' and
            init['seed_derivation'] == 'kernel-python-rl-trainer-sha256-v2' and
            init['model_init_seed'] == model_seed(init['base_seed']), 'fresh initializer differs')
    shape(producer, ('source_git_commit','source_git_clean','source_files','runtime'))
    hex_value(producer['source_git_commit'],40)
    require(type(producer['source_git_clean']) is bool, 'producer clean flag invalid')
    require(type(producer['source_files']) is list and len(producer['source_files']) == 8, 'producer source inventory differs')
    for record, name in zip(producer['source_files'],SOURCE_FILES):
        shape(record, ('path','sha256','bytes')); hex_value(record['sha256'])
        require(record['path'] == name and 0 < integer(record['bytes']) <= 16*MIB, 'producer source pin differs')
    runtime = producer['runtime']
    shape(runtime, ('python_version','python_implementation','platform_system','platform_machine','byte_order',
        'torch_version','device','dtype','deterministic_algorithms','num_threads','num_interop_threads',
        'python_executable','torch_C','torch_cpu_library'))
    require(runtime['byte_order'] == 'little' and runtime['device'] == 'cpu' and runtime['dtype'] == 'torch.float32'
            and runtime['deterministic_algorithms'] is True and type(runtime['num_threads']) is int
            and runtime['num_threads'] == 1 and type(runtime['num_interop_threads']) is int
            and runtime['num_interop_threads'] == 1, 'producer numerical settings differ')
    # Mirror the native loader: four free labels of at most 256 characters and
    # an explicit platform enumeration (fresh_initialization_source.rs validate_metadata).
    for name in ('python_version','python_implementation','platform_machine','torch_version'):
        require(type(runtime[name]) is str and 0 < len(runtime[name]) <= 256, 'producer runtime label invalid')
    require(runtime['platform_system'] in ('Windows','Linux'), 'producer platform is outside the declared CPU contract')
    for name in ('python_executable','torch_C','torch_cpu_library'): file_pin(runtime[name],True)
    shape(target, ('registry','card_db_hash','feature_contract_digest','feature_encoding_digest',
        'features_source_sha256','feature_descriptor_sha256','registry_card_count','card_token_rule'))
    file_pin(target['registry']); hex_value(target['card_db_hash'],16)
    for name in ('feature_contract_digest','feature_encoding_digest','features_source_sha256','feature_descriptor_sha256'):
        hex_value(target[name])
    require(target['card_token_rule'] == 'card-token=id+1; padding=0' and
            feature_transfer == {'expected_feature_contract_digest':target['feature_contract_digest'],
                                  'expected_feature_encoding_digest':target['feature_encoding_digest']},
            'fresh feature transfer differs')
    for index, expected in ((5,target['features_source_sha256']), (6,target['feature_descriptor_sha256']),
                             (7,target['registry']['sha256'])):
        require(producer['source_files'][index]['sha256'] == expected, 'target source pin differs')
    model = manifest['model']
    shape(model, ('architecture','generator_model_config','generator_model_config_sha256'))
    require(model['architecture'] == 'kernel-policy-value-net-8' and
            sha(canonical(model['generator_model_config'])) == model['generator_model_config_sha256'] == CONFIG_SHA,
            'fresh generator configuration differs')
    local, actual = checked_file(descriptor['parameters'],resolve_pin,PAYLOAD_BYTES); dependencies.append(actual)
    with local.open('rb') as stream:
        raw = stream.read(PAYLOAD_BYTES+1)
    require(len(raw) == PAYLOAD_BYTES and sha(raw) == descriptor['parameters']['sha256'] and
            raw[:64] == bytes(64) and all(math.isfinite(v[0]) for v in struct.iter_unpack('<f',raw)),
            'fresh parameter bytes, finite values or +0 padding differ')
    rows, named = [], hashlib.sha256()
    for row in layout():
        values = raw[row['byte_offset']:row['byte_offset']+row['byte_count']]
        rows.append({**row,'sha256':sha(values)})
        name = row['name'].encode('ascii')
        named.update(struct.pack('>I',len(name))+name+struct.pack('>I',len(row['shape'])))
        for dimension in row['shape']: named.update(struct.pack('>Q',dimension))
        named.update(struct.pack('>Q',row['byte_count']//4)+values)
    require(canonical(manifest['parameters']) == canonical(rows), 'fresh tensor layout/hash inventory differs')
    expected_payload = dict(file='parameters.f32le',encoding='ieee-754-binary32-little-endian',
        layout='torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1',
        bytes=PAYLOAD_BYTES,sha256=sha(raw),model_parameter_sha256=named.hexdigest(),
        parameter_layout_sha256=sha(canonical(layout())),tensor_count=33,element_count=PAYLOAD_BYTES//4)
    require(canonical(manifest['payload']) == canonical(expected_payload), 'fresh payload metadata differs')
    anchor = int.from_bytes(raw[4907072:4907076],'little')
    require(canonical(manifest['optimizer_bootstrap']) == canonical({'optimizer_identity':'native-adam-canonical-scorer-bias-gauge-v1',
        'adam_step':0,'moment_initialization':'positive-zero-f32','scorer_bias_anchor_bits':anchor,'value_head_gauge':'none'}),
        'fresh optimizer bootstrap differs')
    origin = dict(schema=ORIGIN_SCHEMA,initialization_manifest_sha256=descriptor['initialization']['sha256'],
        lineage_id=manifest['lineage_id'],initializer=init['identity'],base_seed=init['base_seed'],
        model_init_seed=init['model_init_seed'],seed_derivation=init['seed_derivation'],producer_git_commit=producer['source_git_commit'],
        initial_weights_sha256=sha(raw),initial_model_parameter_sha256=named.hexdigest(),
        parameter_layout_sha256=expected_payload['parameter_layout_sha256'],destination_registry_sha256=target['registry']['sha256'],
        destination_card_db_hash=target['card_db_hash'],destination_card_count=target['registry_card_count'],
        feature_contract_digest=target['feature_contract_digest'],feature_encoding_digest=target['feature_encoding_digest'],
        features_source_sha256=target['features_source_sha256'],feature_descriptor_sha256=target['feature_descriptor_sha256'],
        sampler_identity=SAMPLER)
    validate_origin(origin)
    initial_model = dict(schema='mtg-kernel-actual-play-model/v1',weights_sha256=sha(raw),
        model_parameter_sha256=named.hexdigest(),embedding_table_sha256=sha(raw[:65537*16*4]),
        feature_contract_digest=target['feature_contract_digest'],feature_encoding_digest=target['feature_encoding_digest'],
        card_db_hash=target['card_db_hash'])
    bootstrap = hashlib.sha256()
    atom(bootstrap,'domain',b'mtg-kernel-native-policy-value-train-state-sha256-v1')
    atom(bootstrap,'adam_step_u64be',struct.pack('>Q',0)); atom(bootstrap,'scorer_bias_anchor_f32le',struct.pack('<I',anchor))
    for section in ('parameters','first_moments','second_moments'):
        atom(bootstrap,'section',section.encode('ascii')); atom(bootstrap,'tensor_count',struct.pack('>Q',33))
        for row in layout():
            values = raw[row['byte_offset']:row['byte_offset']+row['byte_count']] if section == 'parameters' else bytes(row['byte_count'])
            for tag, data in (('tensor_ordinal',struct.pack('>Q',row['ordinal'])), ('tensor_name',row['name'].encode('ascii')),
                ('tensor_rank',struct.pack('>Q',len(row['shape']))), ('tensor_shape_u64be',b''.join(struct.pack('>Q',d) for d in row['shape'])),
                ('tensor_element_count',struct.pack('>Q',row['byte_count']//4)), ('tensor_f32le',values)):
                atom(bootstrap,tag,data)
    return dict(origin=origin,dependencies=dependencies,descriptor=descriptor,manifest=manifest,
                initial_model=initial_model,initial_state_sha256=bootstrap.hexdigest())


def validate_inference_identity(identity, source, expected_origin=None):
    origin = identity.get('source_import',{})
    fresh = is_fresh_origin(origin)
    require(identity.get('schema') == 'mtg-kernel-expanded-deck-inference/v'+('2' if fresh else '1'),
            'inference schema/origin differs')
    require(identity.get('checkpoint_sha256') == (source.get('checkpoint') or {}).get('sha256'),
            'inference checkpoint binding differs')
    if expected_origin is not None: require(origin == expected_origin, 'inference initialization origin differs')
    if fresh:
        shape(identity, ('schema','source_import','checkpoint_sha256','model','state_sha256','adam_step',
            'feature_schema_version','feature_registry_version','features_source_sha256','feature_descriptor_sha256'))
        validate_origin(origin)
        integer(identity['adam_step']); hex_value(identity['state_sha256'])
        shape(identity['model'], ('schema','weights_sha256','model_parameter_sha256','embedding_table_sha256',
            'feature_contract_digest','feature_encoding_digest','card_db_hash'))
        require(identity['model']['schema'] == 'mtg-kernel-actual-play-model/v1', 'fresh inference model schema differs')
        # Feature version labels are compile-time constants of the executing
        # build. The BO3 packager binds them to the actual Linux build fields;
        # the ordinary packager and the parity readers leave them to the native
        # loader, so only their shape is checked here and no stale literal can
        # reject a synthetic fixture or a future build.
        for name in ('feature_schema_version', 'feature_registry_version'):
            label = identity[name]
            require(type(label) is str and 0 < len(label) <= 128 and label.isascii() and label.isprintable(),
                    'fresh inference feature version label invalid')
        for name in ('weights_sha256','model_parameter_sha256','embedding_table_sha256'):
            hex_value(identity['model'][name])
        for name in ('feature_contract_digest','feature_encoding_digest'):
            require(identity['model'][name] == origin[name], 'inference feature identity differs')
        require(identity['model']['card_db_hash'] == origin['destination_card_db_hash'], 'inference registry identity differs')
        for name in ('features_source_sha256','feature_descriptor_sha256'):
            require(identity[name] == origin[name], 'inference feature source differs')
    return fresh


def state_hash(state, zero_moments=False):
    hasher = hashlib.sha256()
    atom(hasher,'domain',b'mtg-kernel-native-policy-value-train-state-sha256-v1')
    atom(hasher,'adam_step_u64be',struct.pack('>Q',integer(state['adam_step'],(1 << 31)-1)))
    anchor = integer(state['scorer_bias_anchor_bits'],(1 << 32)-1)
    atom(hasher,'scorer_bias_anchor_f32le',struct.pack('<I',anchor))
    parameters = hashlib.sha256()
    for section in ('parameters','first_moments','second_moments'):
        rows = state[section]
        require(type(rows) is list and len(rows) == 33, 'fresh state requires all33 tensors')
        atom(hasher,'section',section.encode('ascii')); atom(hasher,'tensor_count',struct.pack('>Q',33))
        for row, expected in zip(rows,layout()):
            shape(row, ('name','shape','values'))
            require(row['name'] == expected['name'] and canonical(row['shape']) == canonical(expected['shape']) and
                    type(row['values']) is list and len(row['values'])*4 == expected['byte_count'], 'fresh state tensor layout differs')
            require(all(type(v) is int and 0 <= v < (1 << 32) for v in row['values']), 'fresh state has invalid binary32 bits')
            values = array.array('I',row['values'])
            if sys.byteorder != 'little': values.byteswap()
            data = values.tobytes()
            require(len(data) == expected['byte_count'] and all(math.isfinite(v[0]) for v in struct.iter_unpack('<f',data)),
                    'fresh state tensor has nonfinite values')
            if section == 'second_moments':
                require(all(v[0] >= 0 for v in struct.iter_unpack('<f',data)), 'negative Adam second moment')
            if expected['ordinal'] == 0: require(data[:64] == bytes(64), 'fresh state padding is not +0')
            if expected['ordinal'] == 28:
                require(row['values'] == ([anchor] if section == 'parameters' else [0]), 'fresh scorer gauge differs')
            if section == 'parameters': parameters.update(data)
            elif zero_moments: require(not any(row['values']), 'fresh Adam0 moments are not +0')
            for tag, payload in (('tensor_ordinal',struct.pack('>Q',expected['ordinal'])),
                ('tensor_name',row['name'].encode('ascii')), ('tensor_rank',struct.pack('>Q',len(row['shape']))),
                ('tensor_shape_u64be',b''.join(struct.pack('>Q',d) for d in row['shape'])),
                ('tensor_element_count',struct.pack('>Q',len(row['values']))), ('tensor_f32le',data)):
                atom(hasher,tag,payload)
    require(hasher.hexdigest() == state['state_sha256'], 'fresh full native state hash differs')
    return parameters.hexdigest()


def validate_checkpoint_origin(checkpoint, expected_origin=None, *, expected_weights_sha256=None):
    origin = checkpoint.get('source_import',{})
    fresh = is_fresh_origin(origin)
    require(checkpoint.get('schema') == (CHECKPOINT_SCHEMA if fresh else 'mtg-kernel-expanded-deck-checkpoint/v1'),
            'ordinary checkpoint schema/origin differs')
    if expected_origin is not None: require(origin == expected_origin, 'checkpoint initialization origin differs')
    if fresh:
        validate_origin(origin)
        require(checkpoint['loss_identity'] == 'terminal_reinforce_value/v3' and
                'registry_transfer' not in checkpoint, 'fresh ordinary objective/transfer differs')
        for name in ('feature_contract_digest','feature_encoding_digest'):
            require(checkpoint[name] == origin[name], 'fresh checkpoint feature identity differs')
        require(checkpoint['card_db_hash'] == origin['destination_card_db_hash'], 'fresh checkpoint registry differs')
        for name in ('learning_rate_bits','value_coefficient_bits'):
            value = struct.unpack('<f',struct.pack('<I',integer(checkpoint[name],(1 << 32)-1)))[0]
            require(math.isfinite(value) and value > 0, 'invalid fresh checkpoint optimizer scalar')
        weights = state_hash(checkpoint)
        if expected_weights_sha256 is not None:
            require(weights == expected_weights_sha256, 'checkpoint actual parameter bytes differ from inference model')
    return fresh


def validate_trajectory(document):
    behaviors = document.get('seat_behaviors',[])
    fresh = is_fresh_origin(document.get('source_import')) or any(
        is_fresh_origin(row.get('identity',{}).get('source_import')) for row in behaviors)
    require(document.get('schema') == (TRAJECTORY_SCHEMA if fresh else 'mtg-kernel-expanded-deck-trajectory/v2'),
            'trajectory schema/origin differs')
    if fresh:
        require(len(behaviors) == 2, 'fresh trajectory requires both actors')
        seat = integer(document['episode']['learner_seat'],1)
        for row in behaviors: validate_inference_identity(row['identity'],row['source'])
        require(document['source_import'] == behaviors[seat]['identity']['source_import'] and
                document['behavior_state_sha256'] == behaviors[seat]['identity']['state_sha256'] and
                document['episode']['opponent'] == behaviors[1-seat]['source'], 'fresh trajectory learner/opponent differs')
    else:
        for row in behaviors:
            if 'schema' in row.get('identity',{}): validate_inference_identity(row['identity'],row['source'])
    return fresh


def origin_key(source):
    """Stable key for a configured model source, independent of dict ordering."""
    return sha(canonical(source))


def fresh_origins(config, *, resolve_pin=None):
    """Independently derive the verified origin of every configured fresh source.

    The result maps origin_key(source) to the origin inspect_source computes from
    the pinned initialization manifest and parameter bytes. Consumers bind any
    recorded inference identity for that source to this origin rather than to
    the identity's own self-consistent claim.
    """
    origins = {}
    for source in [config['initial_source'], *[item['source'] for item in config['opponents']]]:
        # Imported play-import pins may carry a byte count; only a fresh descriptor
        # pin is held to the strict two-field shape inside inspect_source.
        reference = source['play_import']
        require(type(reference) is dict and {'path', 'sha256'} <= set(reference) <= {'path', 'sha256', 'bytes'},
                'play import pin fields differ')
        local = Path(reference['path']) if resolve_pin is None else Path(resolve_pin(reference))
        if not local.is_file():
            # The baseline reference contract never opened imported play-import
            # files. An absent fresh descriptor leaves its source unproven, so a
            # fresh trajectory bound to it is rejected at digest time.
            continue
        actual = pin(local, reference['sha256'])
        if 'bytes' in reference:
            require(reference['bytes'] == actual['bytes'], 'play import byte count differs')
        if actual['bytes'] > MIB:
            continue
        descriptor, _ = load_json(local)
        if type(descriptor) is dict and descriptor.get('schema') == SOURCE_SCHEMA:
            origins[origin_key(source)] = inspect_source(source['play_import'], source['feature_transfer'],
                                                         resolve_pin=resolve_pin)['origin']
    return origins


def initial_state(source, inspection, *, resolve_pin=None, equivalent_sources=()):
    """Consume actual inspector arrays, not a fabricated resumable checkpoint."""
    require(source.get('checkpoint') is None, 'Adam0 proof requires a fresh null checkpoint')
    verified = inspect_source(source['play_import'],source['feature_transfer'],resolve_pin=resolve_pin)
    require(type(inspection) is dict and {'path','sha256'} <= set(inspection) <= {'path','sha256','bytes'},
            'inspection file pin fields differ')
    proof_pin = {key:inspection[key] for key in ('path','sha256')}
    local, actual_pin = checked_file(proof_pin,resolve_pin,128*MIB)
    if 'bytes' in inspection: require(inspection['bytes'] == actual_pin['bytes'], 'inspection byte count differs')
    document, raw = load_json(local,128*MIB)
    require(sha(raw) == inspection['sha256'], 'inspection changed during read')
    shape(document, ('schema','source','identity','parameters','first_moments','second_moments',
        'adam_step','scorer_bias_anchor_bits','state_sha256','training_started','strength_claim'))
    require(document['schema'] == 'mtg-kernel-fresh-initialization-inspection/v1' and
            document['source'] in (source,*equivalent_sources) and document['adam_step'] == 0 and
            document['training_started'] is False and document['strength_claim'] is False,
            'native Adam0 inspection/source/scope differs')
    validate_inference_identity(document['identity'],document['source'],verified['origin'])
    parameter_sha = state_hash(document,zero_moments=True)
    origin, model = verified['origin'], document['identity']['model']
    require(parameter_sha == origin['initial_weights_sha256'] and model == verified['initial_model'] and
            document['state_sha256'] == verified['initial_state_sha256'] and
            document['identity']['adam_step'] == 0 and document['identity']['state_sha256'] == document['state_sha256'] and
            document['scorer_bias_anchor_bits'] == verified['manifest']['optimizer_bootstrap']['scorer_bias_anchor_bits'],
            'native Adam0 full state does not match generated parameters')
    # Keep the proof distinct from an ordinary checkpoint in semantic digests.
    return {'schema':'phase1-native-fresh-adam-zero-proof/v1','source_import':origin,
            **{name:document[name] for name in ('state_sha256','adam_step','scorer_bias_anchor_bits',
                                               'parameters','first_moments','second_moments')}}
