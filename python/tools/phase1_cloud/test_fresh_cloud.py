"""Offline transport fixtures only. Synthetic parameters are not seeded models.

The optional frozen-artifact cases below read real producer/inspector evidence;
no test imports torch, executes an engine or modifies frozen evidence.
"""
import array
import copy
import hashlib
from pathlib import Path, PurePosixPath
import struct
import tempfile
import unittest
from common import pin, read, write
from fresh_source import (PAYLOAD_BYTES, SOURCE_SCHEMA, SOURCE_FILES, canonical, sha, layout,
    atom, model_seed, inspect_source, initial_state, state_hash, validate_checkpoint_origin,
    validate_inference_identity, validate_trajectory)


def synthetic_state_digest(state):
    """Make fixture checksums; no claim that this is native execution evidence."""
    digest = hashlib.sha256()
    atom(digest,'domain',b'mtg-kernel-native-policy-value-train-state-sha256-v1')
    atom(digest,'adam_step_u64be',struct.pack('>Q',state['adam_step']))
    atom(digest,'scorer_bias_anchor_f32le',struct.pack('<I',state['scorer_bias_anchor_bits']))
    for section in ('parameters','first_moments','second_moments'):
        atom(digest,'section',section.encode()); atom(digest,'tensor_count',struct.pack('>Q',33))
        for ordinal, row in enumerate(state[section]):
            raw = b''.join(struct.pack('<I',v) for v in row['values'])
            for tag, data in (('tensor_ordinal',struct.pack('>Q',ordinal)), ('tensor_name',row['name'].encode()),
                ('tensor_rank',struct.pack('>Q',len(row['shape']))),
                ('tensor_shape_u64be',b''.join(struct.pack('>Q',v) for v in row['shape'])),
                ('tensor_element_count',struct.pack('>Q',len(row['values']))), ('tensor_f32le',raw)):
                atom(digest,tag,data)
    return digest.hexdigest()


def synthetic_transport_fixture(root, target=None):
    root = Path(root); root.mkdir(parents=True,exist_ok=True)
    target = copy.deepcopy(target) if target is not None else {
        'registry':{'path':'/synthetic/historical/registry.json','sha256':'a'*64}, 'card_db_hash':'b'*16,
        'feature_contract_digest':'c'*64,'feature_encoding_digest':'d'*64,
        'features_source_sha256':'e'*64,'feature_descriptor_sha256':'f'*64,
        'registry_card_count':184,'card_token_rule':'card-token=id+1; padding=0'}
    model_config = {'schema_version':5,'state_dim':219,'action_feature_dim':195,'action_ref_feature_dim':25,
        'object_feature_dim':98,'object_group_count':20,'edge_feature_dim':41,'hidden_dim':64,
        'card_embedding_dim':16,'card_vocab_size':65537,'model_architecture_version':'kernel-policy-value-net-8',
        'feature_schema_version':'actor-relative-v5-python-4','feature_registry_version':'rust-observation-v5-action-v5-registry-4',
        'feature_contract_digest':'bcc808186e40a1ad6aec679d8a386631cb1226379366a632603f0beb95b47396',
        'feature_encoding_digest':'918e57a0796807e84310026de48d30b500813ef37d939462ea85b7255a39111c'}
    payload = bytearray(PAYLOAD_BYTES)
    payload[64:68] = struct.pack('<f',.25); payload[4907072:4907076] = struct.pack('<f',.125)
    payload = bytes(payload); rows, named, parameters = [], hashlib.sha256(), []
    for row in layout():
        raw = payload[row['byte_offset']:row['byte_offset']+row['byte_count']]
        rows.append({**row,'sha256':sha(raw)}); name = row['name'].encode()
        named.update(struct.pack('>I',len(name))+name+struct.pack('>I',len(row['shape'])))
        for dimension in row['shape']: named.update(struct.pack('>Q',dimension))
        named.update(struct.pack('>Q',row['byte_count']//4)+raw)
        parameters.append({'name':row['name'],'shape':row['shape'],'values':[v[0] for v in struct.iter_unpack('<I',raw)]})
    source_files = [{'path':name,'sha256':'1'*64,'bytes':1} for name in SOURCE_FILES]
    for index, value in ((5,target['features_source_sha256']),(6,target['feature_descriptor_sha256']),(7,target['registry']['sha256'])):
        source_files[index]['sha256'] = value
    runtime = dict(python_version='synthetic',python_implementation='synthetic',platform_system='Linux',
        platform_machine='x86_64',byte_order='little',torch_version='synthetic',device='cpu',dtype='torch.float32',
        deterministic_algorithms=True,num_threads=1,num_interop_threads=1)
    for name in ('python_executable','torch_C','torch_cpu_library'):
        runtime[name] = {'path':'/synthetic/historical/'+name,'sha256':'2'*64,'bytes':1}
    anchor = int.from_bytes(payload[4907072:4907076],'little')
    manifest = dict(schema='mtg-kernel-fresh-initialization/v1',lineage_id='synthetic-transport-only',
        initializer=dict(identity='trainer-seeded-v1',authority='Python KernelPolicyValueNet.reset_seeded_parameters',
                         base_seed=0,model_init_seed=model_seed(0),seed_derivation='kernel-python-rl-trainer-sha256-v2'),
        producer=dict(source_git_commit='3'*40,source_git_clean=True,source_files=source_files,runtime=runtime),target=target,
        model=dict(architecture='kernel-policy-value-net-8',generator_model_config=model_config,generator_model_config_sha256=sha(canonical(model_config))),
        payload=dict(file='parameters.f32le',encoding='ieee-754-binary32-little-endian',
            layout='torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1',bytes=PAYLOAD_BYTES,
            sha256=sha(payload),model_parameter_sha256=named.hexdigest(),parameter_layout_sha256=sha(canonical(layout())),tensor_count=33,element_count=PAYLOAD_BYTES//4),
        parameters=rows,optimizer_bootstrap=dict(optimizer_identity='native-adam-canonical-scorer-bias-gauge-v1',adam_step=0,
            moment_initialization='positive-zero-f32',scorer_bias_anchor_bits=anchor,value_head_gauge='none'))
    (root/'parameters.f32le').write_bytes(payload); (root/'initialization.json').write_bytes(canonical(manifest)+b'\n')
    def narrow(path):
        item = pin(path); return {key:item[key] for key in ('path','sha256')}
    descriptor = {'schema':SOURCE_SCHEMA,'initialization':narrow(root/'initialization.json'),'parameters':narrow(root/'parameters.f32le')}
    write(root/'descriptor.json',descriptor)
    transfer = {'expected_feature_contract_digest':target['feature_contract_digest'],
                'expected_feature_encoding_digest':target['feature_encoding_digest']}
    source = {'play_import':narrow(root/'descriptor.json'),'feature_transfer':transfer,'checkpoint':None}
    verified = inspect_source(source['play_import'],transfer)
    zero = [{'name':p['name'],'shape':p['shape'],'values':[0]*len(p['values'])} for p in parameters]
    state = {'adam_step':0,'scorer_bias_anchor_bits':anchor,'parameters':parameters,
             'first_moments':zero,'second_moments':copy.deepcopy(zero)}
    state['state_sha256'] = synthetic_state_digest(state)
    identity = {'schema':'mtg-kernel-expanded-deck-inference/v2','source_import':verified['origin'],
        'checkpoint_sha256':None,'model':verified['initial_model'],'state_sha256':state['state_sha256'],'adam_step':0,
        'feature_schema_version':'actor-relative-v6-python-3','feature_registry_version':'rust-observation-v6-action-v5-registry-3',
        'features_source_sha256':target['features_source_sha256'],'feature_descriptor_sha256':target['feature_descriptor_sha256']}
    inspection = {'schema':'mtg-kernel-fresh-initialization-inspection/v1','source':source,'identity':identity,
                  **state,'training_started':False,'strength_claim':False}
    write(root/'inspection.json',inspection)
    return dict(source=source,origin=verified['origin'],manifest=manifest,payload=payload,identity=identity,state=state,
                inspection=narrow(root/'inspection.json'))


class FreshTransportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(); cls.root = Path(cls.temp.name)
        cls.fixture = synthetic_transport_fixture(cls.root/'fixture')

    @classmethod
    def tearDownClass(cls): cls.temp.cleanup()

    def test_explicit_inspector_proof_has_no_checkpoint_schema(self):
        f = self.fixture
        proof = initial_state(f['source'],f['inspection'])
        self.assertEqual(proof['schema'],'phase1-native-fresh-adam-zero-proof/v1')
        self.assertEqual(proof['state_sha256'],f['state']['state_sha256'])

    def test_descriptor_duplicates_and_unknown_fields_are_rejected(self):
        f = self.fixture; original = read(f['source']['play_import']['path'])
        for index, raw in enumerate((b'{"schema":"x","schema":"y"}',canonical({**original,'surprise':True}))):
            path = self.root/f'bad-{index}.json'; path.write_bytes(raw)
            with self.assertRaises(ValueError):
                inspect_source({'path':str(path),'sha256':sha(raw)},f['source']['feature_transfer'])

    def test_relocation_preserves_manifest_payload_and_origin(self):
        from pack import Packager
        from semantic import validated_map
        f = self.fixture; p = Packager(self.root/'package','/opt/phase1/fresh-test')
        moved = p.model(f['source'])
        manifest = {'schema':'phase1-cloud-payload/v1','remote_root':str(p.remote),
                    'model_source_relocations':p.relocations,'files':copy.deepcopy(p.files)}
        path = p.output/'payload/manifest.json'; write(path,manifest)
        rows = validated_map(pin(path),{'initial_source':moved,'opponents':[]})
        self.assertEqual(rows[0]['original'],f['source'])
        descriptor = read(p.output/'payload'/PurePosixPath(moved['play_import']['path']).relative_to(p.remote).as_posix())
        for key in ('initialization','parameters'):
            self.assertEqual(descriptor[key]['sha256'],read(f['source']['play_import']['path'])[key]['sha256'])

    def test_wrong_origin_schema_and_negative_variance_rejected(self):
        f = self.fixture; bad = copy.deepcopy(f['identity']); bad['schema']='mtg-kernel-expanded-deck-inference/v1'
        with self.assertRaises(ValueError): validate_inference_identity(bad,f['source'])
        state = copy.deepcopy(f['state']); state['second_moments'][1]['values'][0]=0xbf800000
        state['state_sha256']=synthetic_state_digest(state)
        with self.assertRaises(ValueError): state_hash(state)
        state['adam_step']=1 << 31
        with self.assertRaises(ValueError): state_hash(state)

    def test_mixed_imported_learner_requires_v3_without_relabeling_ancestry(self):
        f = self.fixture
        imported = {'source':f['source'],'identity':{'schema':'mtg-kernel-expanded-deck-inference/v1',
                    'source_import':{'legacy':'synthetic'},'checkpoint_sha256':None,'state_sha256':'4'*64}}
        trajectory = {'schema':'mtg-kernel-expanded-deck-trajectory/v3','source_import':imported['identity']['source_import'],
            'behavior_state_sha256':'4'*64,'episode':{'learner_seat':0,'opponent':f['source']},
            'seat_behaviors':[imported,{'source':f['source'],'identity':f['identity']}]}
        self.assertTrue(validate_trajectory(trajectory))
        trajectory['schema']='mtg-kernel-expanded-deck-trajectory/v2'
        with self.assertRaises(ValueError): validate_trajectory(trajectory)


if __name__ == '__main__': unittest.main()
