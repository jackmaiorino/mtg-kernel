"""Dry-run manifests for the line (a) calibration, screen training and screen evaluation.

Engineering only. This module turns pinned inputs into hash-addressable
manifests; it never dispatches, reads outcomes, or selects a member, seed,
weight or threshold. A manifest is launchable only when every roster slot,
engine contract, byte worksheet and throughput record is bound; otherwise it
lists exactly what blocks it and the launcher refuses it.

Counts and rules come from the proposal's v0.2 section
(collab/CODEX-G115-OPPONENT-PORTFOLIO-PROPOSAL-20260926.md, 2026-09-27 10:18):
seven canonical learner decks in fixed order, opponent deck (i + j) mod 7,
32 blocks, both learner seats, 16 equal-weight members, integer win weight
20/n_r for rotation r = j mod 7 (denominator 31,360 per endpoint).
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path

import g115_line_a_seeds_v1 as seeds

SCHEMAS = {
    'calibration': 'g115-line-a-calibration-manifest/v1',
    'screen-training': 'g115-line-a-screen-training-manifest/v1',
    'screen-evaluation': 'g115-line-a-screen-evaluation-manifest/v1',
}
DECK_PACKET_SCHEMA = 'g115-line-a-deck-packet/v1'
DECKS = ('Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire')
DECK_MAP_RULE = 'learner deck index i, block index j: opponent deck index (i + j) mod 7'
BLOCKS = 32
SEATS = (0, 1)
ROSTER_SIZE = 16
ROSTER_ROLES = (
    'V3', 'D3 wrapper',
    'refresh-034/anchor-0', 'refresh-034/anchor-1', 'refresh-034/historical-0', 'refresh-034/historical-1',
    'refresh-034/current-0', 'refresh-034/current-1', 'refresh-034/exploiter-0', 'refresh-034/exploiter-1',
    'exploiter-v3b/arm1/run-0', 'exploiter-v3b/arm1/run-1', 'exploiter-v3b/arm1/run-2',
    'exploiter-v3b/arm2/run-0', 'exploiter-v3b/arm2/run-1', 'exploiter-v3b/arm2/run-2',
)
ROTATION_BLOCKS = tuple(sum(1 for j in range(BLOCKS) if j % 7 == r) for r in range(7))  # (5,5,5,5,4,4,4)
WEIGHTS = tuple(20 // n for n in ROTATION_BLOCKS)  # integer win weight 20/n_r: (4,4,4,4,5,5,5)
DENOMINATOR = 31360
CALIBRATION_BO3 = 7168
TRAINING_GAMES = 8000
SCREEN_EVALUATION_BO3 = 35840

TEMPLATE = dict(path='E:/mtg-postboard-campaign-20260921/control-variance-001/replica-1/configs/a.json',
                sha256='4352f9cc6bb29e382ea8543ae6be59d547ec16619f716f604bfae24a715e158e')
G115_CHECKPOINT_SHA256 = '88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1'
A48_CHECKPOINT_SHA256 = 'beb86b4116c6eaa8406f12eb391adde1a637e063375740ea0b9812251bcff4a4'
SEARCH_DESCRIPTOR = dict(path='E:/mtg-g115-lineage-20260923/d3-search-descriptor-reviewed.json',
                         sha256='5eb1d55d13b78b341f8ff0c4df2589f8ee725974dc9fcadd691db6e12b2133d7')
# The frozen D3 yardstick source of V3 (d3-panel-preparation-001.json, eaca43dc). Its transfer
# envelope is rebound to the pinned evaluator build at binding time, as g115_d3_payload_v1 does.
V3_SOURCE = dict(
    checkpoint=None,
    feature_transfer=dict(
        expected_feature_contract_digest='9319fbd41e6b42ec90d565c13f3b4f75a3898dafe3816387453c08419ba9cc68',
        expected_feature_encoding_digest='c4662291ca9a75525b51b51f3b5d512671340c05b69fd33fb0827c0c8af70a2b'),
    play_import=dict(path='E:/mtg-meta-recovery-20260921/entropy-scorer-qualification-003/adapter/play-import-source.json',
                     sha256='9b5364158496c21adf02fcbf0c02aac5c5a94b424166c02c226e8f08f0779661'))
# Frozen D3 whole-match options (d3-panel-preparation-001.json, every job).
MATCH_OPTIONS = dict(max_physical_decisions=4000, max_physical_games=6, max_policy_steps=40000,
                     opening_protocol='keep_seven_v2')
# Proposed: sources index 0 plays first in game one, so learner seats 0 and 1 balance play and draw.
GAME_ONE_CHOOSER = 0
CAPS = dict(calibration=12_000_000_000, screen=48_000_000_000, total=60_000_000_000)
PENDING_CONTRACTS = (
    'C1 episode schedule rule (CLAUDE #445)',
    'C2 screen-training config materialization and loading receipt (CLAUDE #445)',
    'C3 one BO3 per evaluator request and the pinned yardstick evaluator build (CLAUDE #445)',
    'C4 transport allowlist extension (CLAUDE #445)',
    'C5 game-one chooser 0 and the template-derived deck packet',
    'lane design verdict (FABLE-QUEUE.md, Opus lane line-a-launcher)',
)
UNSUPPORTED_TRAINING = ('public_feature_training_v1 accepts only V4 opponents '
                        '(expanded_deck_training_v1/public_features.rs:207-210); owner not yet named (CLAUDE #445)')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def canonical_sha256(value):
    return seeds.canonical_sha256(value)


def file_sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_template(path=TEMPLATE['path']):
    raw = Path(path).read_bytes()
    require(hashlib.sha256(raw).hexdigest() == TEMPLATE['sha256'], 'Template bytes differ from the pin')
    return json.loads(raw)


def check_template_shape(template):
    updates = template['updates']
    require(len(updates) == seeds.UPDATES and all(len(u) == seeds.GAMES_PER_UPDATE for u in updates),
            'Template is not 200 updates of 10 games')
    return [episode for update in updates for episode in update]


def deck_packet(template):
    """Canonical registrations and fixed game-two decks, read from the template's own episodes."""
    decks, postboard = {}, {}
    for episode in check_template_shape(template):
        labels = [deck['label'] for deck in episode['registered']]
        for deck in episode['registered']:
            if deck['label'] in DECKS:
                require(decks.setdefault(deck['label'], deck) == deck, 'Registration differs: ' + deck['label'])
        if episode['postboard'] and all(label in DECKS for label in labels):
            for seat in SEATS:
                key = labels[seat] + '|' + labels[1 - seat]
                chosen = episode['selected'][seat]
                require(postboard.setdefault(key, chosen) == chosen, 'Game-two deck differs: ' + key)
    require(set(decks) == set(DECKS), 'Template lacks a canonical registration')
    require(set(postboard) == {x + '|' + y for x in DECKS for y in DECKS}, 'Template lacks a canonical pairing')
    return dict(schema=DECK_PACKET_SCHEMA, source=TEMPLATE, decks={label: decks[label] for label in DECKS},
                postboard={key: postboard[key] for key in sorted(postboard)})


def opponent_deck(learner_index, block):
    return DECKS[(learner_index + block) % 7]


def evaluation_jobs(prefix, labelled_seeds):
    """Block-major enumeration of one endpoint's 7,168 BO3 cells."""
    labels = list(labelled_seeds)
    require(len(labels) == BLOCKS, 'Evaluation needs 32 seed blocks')
    jobs = []
    for j, label in enumerate(labels):
        for member in range(ROSTER_SIZE):
            for i, deck in enumerate(DECKS):
                for seat in SEATS:
                    jobs.append(dict(id=f'{prefix}-b{j:02d}-m{member:02d}-d{i}-s{seat}', block=j, seed_label=label,
                                     seed=labelled_seeds[label], member=member, learner_deck=deck,
                                     opponent_deck=opponent_deck(i, j), learner_seat=seat, rotation=j % 7,
                                     weight=WEIGHTS[j % 7]))
    return jobs


def native_match(job, packet):
    """The public_feature_evaluation_v1 match for one job; the learner sits at its seat index."""
    by_seat = [job['learner_deck'], job['opponent_deck']] if job['learner_seat'] == 0 else \
        [job['opponent_deck'], job['learner_deck']]
    config = dict(deck_ids=by_seat, seed=job['seed'], game_one_chooser=GAME_ONE_CHOOSER, **MATCH_OPTIONS)
    return dict(config=config, registered=[packet['decks'][deck] for deck in by_seat],
                postboard=[packet['postboard'][by_seat[0] + '|' + by_seat[1]],
                           packet['postboard'][by_seat[1] + '|' + by_seat[0]]])


def evaluation_roster(g115_source, panel_imports=None):
    """16 evaluation identities in v0.2 order; unbound slots stay pending, never replaced."""
    roster = [dict(index=0, role=ROSTER_ROLES[0], status='bound',
                   source=dict(kind='legacy', source=V3_SOURCE, v3_forced_actions=True,
                               v3_spell_target_reference_adapter=True),
                   note='transfer envelope rebound to the pinned evaluator build at binding time'),
              dict(index=1, role=ROSTER_ROLES[1], status='bound', descriptor=SEARCH_DESCRIPTOR,
                   source=dict(kind='information_set_search_v3', source=g115_source,
                               descriptor=SEARCH_DESCRIPTOR['sha256']),
                   note='descriptor file content replaces its hash at binding time')]
    members = {m['roster_index']: m for m in (panel_imports or {}).get('members', [])}
    for index in range(2, ROSTER_SIZE):
        member = members.get(index)
        if member and member.get('status') == 'playable' and member.get('model_source'):
            require(member['label'] == ROSTER_ROLES[index], 'Panel import order differs at index %d' % index)
            # CODEX #520 item 5: true/true authority adapters for explicitly V3-transferred members.
            roster.append(dict(index=index, role=ROSTER_ROLES[index], status='bound',
                               source=dict(kind='legacy', source=member['model_source'],
                                           v3_forced_actions=True, v3_spell_target_reference_adapter=True)))
        else:
            reason = member['status'] if member else 'no panel import manifest supplied'
            roster.append(dict(index=index, role=ROSTER_ROLES[index], status='pending', reason=reason))
    return roster


def training_roster(evaluation):
    """Treatment opponents per roster slot; only V4 sources are trainable at 500cfae9."""
    result = []
    for slot in evaluation:
        if slot['index'] == 1:
            result.append(dict(index=1, role=slot['role'], status='pending',
                               reason='D3 wrapper as training opponent: opus-search-opponent lane (opponent_search pin)'))
        else:
            reason = UNSUPPORTED_TRAINING if slot['status'] == 'bound' else slot['reason']
            result.append(dict(index=slot['index'], role=slot['role'], status='pending', reason=reason))
    return result


def rotation_check(jobs, members):
    weights = sum(job['weight'] for job in jobs)
    require(weights == DENOMINATOR * members // ROSTER_SIZE, 'Integer weights do not sum to the denominator')


def byte_worksheet(cap, measured=None):
    if measured is None:
        return dict(cap_bytes=cap, projected_bytes=None, status='unmeasured: qualification pending')
    return dict(cap_bytes=cap, projected_bytes=measured, status='measured')


def blocking(roster_slots, worksheet):
    reasons = ['slot %s (%s) pending: %s' % (s.get('index', s.get('id')), s['role'], s['reason'])
               for s in roster_slots if s['status'] != 'bound']
    if worksheet['status'] != 'measured':
        reasons.append('byte worksheet ' + worksheet['status'])
    reasons.append('throughput evidence absent for the selected placement')
    reasons.extend('pending: ' + item for item in PENDING_CONTRACTS)
    return reasons


def provenance(panel_imports_sha256):
    here = Path(__file__).resolve().parent
    return dict(builder=dict(path='python/tools/g115_line_a_manifest_v1.py',
                             sha256=file_sha256(here / 'g115_line_a_manifest_v1.py')),
                seed_consumer=dict(path='python/tools/g115_line_a_seeds_v1.py',
                                   sha256=file_sha256(here / 'g115_line_a_seeds_v1.py')),
                panel_imports_sha256=panel_imports_sha256)


def finish(manifest, panel_imports_sha256):
    manifest['provenance'] = provenance(panel_imports_sha256)
    manifest['manifest_sha256'] = canonical_sha256(manifest)
    return manifest


def calibration_manifest(seed_manifest, packet, roster, learner_source, imports_sha256=None):
    labelled = seeds.calibration_seeds(seed_manifest)
    jobs = evaluation_jobs('cal', labelled)
    require(len(jobs) == CALIBRATION_BO3, 'Calibration count differs')
    rotation_check(jobs, ROSTER_SIZE)
    worksheet = byte_worksheet(CAPS['calibration'])
    reasons = blocking(roster, worksheet)
    return finish(dict(
        schema=SCHEMAS['calibration'], dry_run=True, launchable=not reasons, blocking=reasons,
        seed_manifest=dict(path=seeds.SEED_MANIFEST_PATH, sha256=seeds.SEED_MANIFEST_SHA256),
        decks=list(DECKS), deck_map_rule=DECK_MAP_RULE, deck_packet_sha256=canonical_sha256(packet),
        learner=dict(role='untouched g115', source=learner_source),
        roster=roster, seeds=[dict(label=label, seed=value) for label, value in labelled.items()],
        match_options=dict(MATCH_OPTIONS, game_one_chooser=GAME_ONE_CHOOSER),
        scoring=dict(rotation_blocks=list(ROTATION_BLOCKS), weights=list(WEIGHTS), denominator=DENOMINATOR),
        counts=dict(bo3=len(jobs), members=ROSTER_SIZE, decks=len(DECKS), blocks=BLOCKS, seats=len(SEATS),
                    per_member=len(jobs) // ROSTER_SIZE),
        job_list_sha256=canonical_sha256(jobs), byte_budget=worksheet,
        scratch_root='D:/e-scratch/g115-line-a-calibration/'), imports_sha256)


def training_config(template, schedule, block, arm, opponents=None):
    """Template copy: only episode seed, id and (treatment) opponent change."""
    config = copy.deepcopy(template)
    for index, episode in enumerate(check_template_shape(config)):
        update, slot = divmod(index, seeds.GAMES_PER_UPDATE)
        episode['seed'] = schedule['seeds'][index]
        episode['id'] = f'la-b{block:02d}-i{update:03d}-s{slot:02d}'
        if arm == 'treatment':
            episode['opponent'] = copy.deepcopy(opponents[index % ROSTER_SIZE])
    return config


def config_bytes(config):
    return json.dumps(config, separators=(',', ':'), ensure_ascii=True, allow_nan=False).encode('ascii')


def control_opponent_counts(template):
    counts = {}
    for episode in check_template_shape(template):
        key = episode['opponent']['checkpoint']['sha256']
        counts[key] = counts.get(key, 0) + 1
    return counts


def screen_training_manifest(seed_manifest, template, training_slots, imports_sha256=None):
    schedules = seeds.schedules(seed_manifest)
    counts = control_opponent_counts(template)
    require(counts == {G115_CHECKPOINT_SHA256: 1000, A48_CHECKPOINT_SHA256: 1000},
            'Control template is not 1,000 g115 and 1,000 A48')
    runs = []
    for block, label in enumerate(seeds.TRAINING_LABELS):
        schedule = schedules[label]
        for arm in ('treatment', 'control'):
            run = dict(id=f'la-screen-b{block:02d}-{arm}', block_label=label, arm=arm,
                       schedule_sha256=canonical_sha256(schedule), episodes=seeds.EPISODES,
                       updates=seeds.UPDATES, games_per_update=seeds.GAMES_PER_UPDATE,
                       initial_checkpoint_sha256=G115_CHECKPOINT_SHA256)
            if arm == 'control':
                run.update(opponents=dict(g115=1000, a48=1000, rule='template assignments unchanged'),
                           config_sha256=hashlib.sha256(config_bytes(
                               training_config(template, schedule, block, arm))).hexdigest())
            else:
                run.update(opponents=dict(rule='flattened episode e plays roster index e mod 16',
                                          per_member=seeds.EPISODES // ROSTER_SIZE),
                           config_sha256=None)
            runs.append(run)
    games = sum(run['episodes'] for run in runs)
    require(games == TRAINING_GAMES, 'Training game count differs')
    worksheet = byte_worksheet(CAPS['screen'])
    reasons = blocking(training_slots, worksheet)
    return finish(dict(
        schema=SCHEMAS['screen-training'], dry_run=True, launchable=not reasons, blocking=reasons,
        seed_manifest=dict(path=seeds.SEED_MANIFEST_PATH, sha256=seeds.SEED_MANIFEST_SHA256),
        template=TEMPLATE, schedule_rule=seeds.SCHEDULE_RULE,
        changed_fields=['updates[*][*].seed', 'updates[*][*].id', 'updates[*][*].opponent (treatment only)'],
        loading_receipt=('update 0 before_state_sha256 and every trajectory optimizer_state_sha256 must be '
                         'equal across the matched arms of a block and match the pinned g115 state'),
        roster=training_slots, runs=runs, counts=dict(runs=len(runs), games=games),
        seeds=[dict(label=label, seed=schedules[label]['block_seed']) for label in seeds.TRAINING_LABELS],
        byte_budget=worksheet, scratch_root='D:/e-scratch/g115-line-a-screen/'), imports_sha256)


def screen_evaluation_manifest(seed_manifest, packet, roster, learner_source, imports_sha256=None):
    labelled = seeds.screen_evaluation_seeds(seed_manifest)
    per_endpoint = evaluation_jobs('scr', labelled)
    rotation_check(per_endpoint, ROSTER_SIZE)
    endpoints = [dict(id='g115', role='untouched g115, evaluated once and shared', status='bound',
                      source=learner_source)]
    for block in range(2):
        for arm in ('treatment', 'control'):
            endpoints.append(dict(id=f'la-screen-b{block:02d}-{arm}', role='trained endpoint', status='pending',
                                  reason='produced by the screen training run'))
    total = len(per_endpoint) * len(endpoints)
    require(total == SCREEN_EVALUATION_BO3, 'Screen evaluation count differs')
    worksheet = byte_worksheet(CAPS['screen'])
    reasons = blocking(roster + endpoints, worksheet)
    return finish(dict(
        schema=SCHEMAS['screen-evaluation'], dry_run=True, launchable=not reasons, blocking=reasons,
        seed_manifest=dict(path=seeds.SEED_MANIFEST_PATH, sha256=seeds.SEED_MANIFEST_SHA256),
        seed_rule='unsigned big-endian first 8 bytes of SHA256(ASCII("g115-line-a-seed-v2|" + label))',
        decks=list(DECKS), deck_map_rule=DECK_MAP_RULE, deck_packet_sha256=canonical_sha256(packet),
        roster=roster, endpoints=endpoints,
        seeds=[dict(label=label, seed=value) for label, value in labelled.items()],
        match_options=dict(MATCH_OPTIONS, game_one_chooser=GAME_ONE_CHOOSER),
        scoring=dict(rotation_blocks=list(ROTATION_BLOCKS), weights=list(WEIGHTS), denominator=DENOMINATOR),
        counts=dict(bo3=total, endpoints=len(endpoints), per_endpoint=len(per_endpoint)),
        job_list_sha256=canonical_sha256(per_endpoint), byte_budget=worksheet,
        scratch_root='D:/e-scratch/g115-line-a-screen/'), imports_sha256)


def build(seed_manifest, template, panel_imports=None, imports_sha256=None):
    packet = deck_packet(template)
    require(template['source']['checkpoint']['sha256'] == G115_CHECKPOINT_SHA256, 'Template does not start from g115')
    roster = evaluation_roster(template['source'], panel_imports)
    learner = dict(kind='legacy', source=template['source'], v3_forced_actions=False)
    return dict(deck_packet=packet,
                calibration=calibration_manifest(seed_manifest, packet, roster, learner, imports_sha256),
                screen_training=screen_training_manifest(seed_manifest, template, training_roster(roster),
                                                         imports_sha256),
                screen_evaluation=screen_evaluation_manifest(seed_manifest, packet, roster, learner,
                                                             imports_sha256))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--seed-manifest', type=Path, default=Path(seeds.SEED_MANIFEST_PATH))
    parser.add_argument('--template', type=Path, default=Path(TEMPLATE['path']))
    parser.add_argument('--panel-imports', type=Path, help='mtg-kernel-line-a-panel-imports/v1 manifest')
    parser.add_argument('--out', type=Path, help='Fresh directory for the dry-run manifests')
    args = parser.parse_args()
    imports = imports_sha256 = None
    if args.panel_imports:
        imports = json.loads(args.panel_imports.read_bytes())
        imports_sha256 = file_sha256(args.panel_imports)
        require(imports['schema'] == 'mtg-kernel-line-a-panel-imports/v1', 'Wrong panel import schema')
    result = build(seeds.load_seed_manifest(args.seed_manifest), load_template(args.template), imports,
                   imports_sha256)
    if args.out:
        args.out.mkdir(parents=True)
        for name, value in result.items():
            (args.out / (name + '.json')).write_bytes(seeds.canonical_bytes(value))
    summary = {name: dict(sha256=canonical_sha256(value) if name == 'deck_packet' else value['manifest_sha256'],
                          counts=value.get('counts'), launchable=value.get('launchable'),
                          blocking=len(value.get('blocking', [])))
               for name, value in result.items()}
    summary['panel_imports_sha256'] = imports_sha256
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
