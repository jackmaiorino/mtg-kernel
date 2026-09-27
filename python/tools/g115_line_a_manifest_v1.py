"""Dry-run manifests for the line (a) calibration, screen training and screen evaluation.

Engineering only. This module turns pinned inputs into hash-addressable
manifests; it never dispatches, reads outcomes, or selects a member, seed,
weight, exposure or threshold. A manifest is launchable only when the roster
is frozen and every member source, exposure table, engine contract, byte
worksheet and throughput record is bound; otherwise it lists exactly what
blocks it and the launcher refuses it.

Rules come from the proposal's v0.2 section as corrected by the director's
ruling of 2026-09-27 (collab/DIRECTOR-RULINGS-20260927.md, R2 to R4, R7, R10):
seven canonical learner decks in fixed order, opponent deck (i + j) mod 7,
32 blocks, both learner seats; full-scope members play every cell (448 BO3
per evaluated endpoint) and the archival roles only Rally-opponent cells
(64); composition E carries the archival roles, the staged fallback B does
not; the descriptive holdout joins the screen evaluation only. Member order,
exposure and weights are read from the frozen roster manifest, never chosen.
"""
import argparse
import copy
from fractions import Fraction
import hashlib
import json
from pathlib import Path

import g115_line_a_seeds_v1 as seeds

SCHEMAS = {
    'calibration': 'g115-line-a-calibration-manifest/v1',
    'screen-training': 'g115-line-a-screen-training-manifest/v1',
    'screen-evaluation': 'g115-line-a-screen-evaluation-manifest/v1',
}
ROSTER_SCHEMA = 'g115-line-a-roster/v1'
EXPOSURE_SCHEMA = 'g115-line-a-exposure-table/v1'
DECK_PACKET_SCHEMA = 'g115-line-a-deck-packet/v1'
DECKS = ('Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire')
RALLY = DECKS.index('Rally')
DECK_MAP_RULE = 'learner deck index i, block index j: opponent deck index (i + j) mod 7'
BLOCKS = 32
SEATS = (0, 1)
ROLES = {'v3': 'full', 'd3-wrapper': 'full', 'recent': 'full', 'population': 'rally', 'august': 'rally',
         'holdout': 'full'}
CANONICAL_ROLES = ('v3', 'd3-wrapper', 'recent')
ARCHIVAL_ROLES = ('population', 'august')
COMPOSITIONS = ('E', 'B')
# Whole-role cardinalities of version 1 (R2): every declared identity, or the role defers whole (R3).
ROLE_CARDINALITY = {'v3': 1, 'd3-wrapper': 1, 'recent': 4, 'population': 4, 'august': 4}
PER_FULL_MEMBER = len(DECKS) * BLOCKS * len(SEATS)  # 448
PER_RALLY_MEMBER = BLOCKS * len(SEATS)  # 64: one learner deck per block faces Rally

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
# Recent role (R2(c)): control-variance-002 replicas 4 and 5, both arms, pins from their training audits.
RECENT_ENDPOINTS = {
    'recent/r4-a': ('replica-4', 'a', '57bd7bb3f93f093acafc3792c5588f20b52d262135cdc009137d52e0093b85f9'),
    'recent/r4-b': ('replica-4', 'b', 'f098a5dbaffe69f48e018e229a4bade3051df5b9e0aa8c7438b0f9022267c091'),
    'recent/r5-a': ('replica-5', 'a', '6c4c9296148b06696fcf013800a19ac2dc7723d56af32c870da1839707bfec5e'),
    'recent/r5-b': ('replica-5', 'b', 'aa85976c8c911d8f24f14576c1aa2501f08229f45010531d8821410f51fc702f'),
}
POPULATION_SLOTS = ('anchor-0', 'anchor-1', 'current-0', 'current-1')
AUGUST_SLOTS = ('historical-0', 'historical-1', 'exploiter-0', 'exploiter-1')
# Draft lineage labels from the ruling's refresh-034 evidence (DIRECTOR-RULINGS-20260927.md, Evidence).
ARCHIVAL_LINEAGE = {'anchor-0': 'ladder-pilot pool3 primary 920012', 'anchor-1': 'scaled-selfplay 970002',
                    'current-0': 'population-v2 cycle-2 975001',
                    'current-1': 'population-v2 cycle-3 real-attempt-003',
                    'historical-0': 'de-novo 971221', 'historical-1': 'de-novo 971223',
                    'exploiter-0': 'de-novo 971222', 'exploiter-1': 'de-novo 971221'}
# Frozen D3 whole-match options (d3-panel-preparation-001.json, every job).
MATCH_OPTIONS = dict(max_physical_decisions=4000, max_physical_games=6, max_policy_steps=40000,
                     opening_protocol='keep_seven_v2')
# Proposed: sources index 0 plays first in game one, so learner seats 0 and 1 balance play and draw.
GAME_ONE_CHOOSER = 0
CAPS = dict(calibration=12_000_000_000, screen=48_000_000_000, total=60_000_000_000)
PENDING_CONTRACTS = (
    'C2 loading receipt must trace the pre-update model, moments and age to the pinned g115 input (CODEX #523)',
    'C3 one new integrated yardstick executable, pinned before calibration (CODEX #523)',
    'C5 game-one chooser 0 and the template-derived deck packet (not yet countersigned)',
    'lane design verdict (FABLE-QUEUE.md, Opus lane line-a-launcher)',
)
NOT_TRAINABLE = {
    'v3': 'public_feature_training_v1 accepts only V4 opponents and has no V3 adapters '
          '(public_features.rs:207-210); ownerless-blocker entry filed by Codex (CODEX #523)',
    'd3-wrapper': 'D3 wrapper as training opponent: opus-search-opponent lane; episode opponent is the g115 source '
                  'plus opponent_search = the reviewed descriptor pin (CLAUDE #449, CODEX #521); code and '
                  'collector qualification pending',
    'recent': 'public-input checkpoints are evaluation-only; the training opponent loader reads only ordinary '
              'expanded-deck checkpoints (expanded_deck_training_v1.rs:862-875); owner not yet named',
    'population': 'V3-transfer archival import; public_feature_training_v1 accepts only V4 opponents',
    'august': 'V3-transfer archival import; public_feature_training_v1 accepts only V4 opponents',
}


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


def opponent_label(episode):
    return episode['registered'][1 - episode['learner_seat']]['label']


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


def require_complete_pins(value, where):
    """A bound source names every file by path and full SHA256 (CODEX #524 C6)."""
    if isinstance(value, dict):
        if 'path' in value or 'sha256' in value:
            digest = value.get('sha256')
            require(bool(value.get('path')) and isinstance(digest, str) and len(digest) == 64 and
                    all(c in '0123456789abcdef' for c in digest), 'Incomplete pin in ' + where)
        for item in value.values():
            require_complete_pins(item, where)
    elif isinstance(value, list):
        for item in value:
            require_complete_pins(item, where)


def pinned_ref(value):
    return isinstance(value, dict) and set(value) == {'path', 'sha256'}


def exact_rational(value, member_id):
    """'p/q' text or {'numerator': p, 'denominator': q} integers (the form CODEX #525 emits); never a float."""
    if isinstance(value, dict):
        require(set(value) == {'numerator', 'denominator'} and type(value['numerator']) is int and
                type(value['denominator']) is int and value['denominator'] > 0,
                'Weight must be an exact rational: ' + member_id)
        return Fraction(value['numerator'], value['denominator'])
    require(isinstance(value, str) and '/' in value, 'Weight must be an exact rational p/q: ' + member_id)
    return Fraction(value)


def validate_weights(roster):
    """Exact rational yardstick weights per composition over its non-holdout members, summing to one."""
    weights = roster['yardstick_weights']
    require(isinstance(weights, dict) and str(weights.get('normalization', '')).strip(),
            'Frozen roster needs yardstick weights with their deck and cell normalization declared')
    for composition in COMPOSITIONS:
        table = weights.get(composition)
        require(isinstance(table, dict), 'Yardstick weights missing for composition ' + composition)
        members = {m['id'] for m in composition_members(roster, composition)}
        require(set(table) == members, 'Yardstick weights must cover exactly the composition members, no holdout')
        values = []
        for member_id, value in table.items():
            values.append(exact_rational(value, member_id))
            require(values[-1] > 0, 'Weight must be positive: ' + member_id)
        require(sum(values) == 1, 'Yardstick weights of composition %s do not sum to one' % composition)


def validate_roster(roster):
    require(roster['schema'] == ROSTER_SCHEMA, 'Wrong roster schema')
    ids = [member['id'] for member in roster['members']]
    require(len(ids) == len(set(ids)), 'Duplicate roster member id')
    for member in roster['members']:
        require(member['role'] in ROLES and member['scope'] == ROLES[member['role']],
                'Role or scope differs from the ruling: ' + member['id'])
        require(isinstance(member.get('lineage'), str) and member['lineage'].strip(),
                'Lineage id required: ' + member['id'])
        for use in ('evaluation', 'training'):
            require(member[use]['status'] in ('bound', 'pending'), 'Unknown %s status: %s' % (use, member['id']))
            require(member[use]['status'] == 'bound' or member[use].get('reason'), 'Pending without a reason')
        if member['evaluation']['status'] == 'bound':
            require_complete_pins(member['evaluation']['source'], member['id'])
        if member['training']['status'] == 'bound':
            require(member['role'] != 'holdout', 'The holdout never receives training exposure')
            fields = member['training'].get('episode_fields')
            require(isinstance(fields, dict) and 'opponent' in fields, 'Bound training source needs episode fields')
            require_complete_pins(fields, member['id'])
    for role, count in ROLE_CARDINALITY.items():
        require(sum(m['role'] == role for m in roster['members']) == count,
                'Role %s must carry all %d declared identities' % (role, count))
    require(sum(m['role'] == 'holdout' for m in roster['members']) <= 1, 'At most one holdout')
    table = {role: [m['id'] for m in roster['members'] if m['role'] == role] for role in ROLES}
    require(roster.get('roles') == table, 'Roster role table differs from its members')
    if roster['frozen']:
        require(roster.get('template') == TEMPLATE, 'Frozen roster must bind the template hash')
        tables = roster['exposure_tables']
        require(set(tables) == set(COMPOSITIONS) and all(pinned_ref(t) for t in tables.values()),
                'Frozen roster must bind both exposure tables by hash')
        validate_weights(roster)
    return roster


def composition_members(roster, composition, holdout=False):
    """E carries every declared role; staged B drops both archival roles whole."""
    require(composition in COMPOSITIONS, 'Unknown composition')
    members = [m for m in roster['members'] if m['role'] != 'holdout' and
               (composition == 'E' or m['role'] not in ARCHIVAL_ROLES)]
    if holdout:
        members += [m for m in roster['members'] if m['role'] == 'holdout']
    return members


def evaluation_jobs(prefix, labelled_seeds, members):
    """Block-major cells of one endpoint; Rally-scope members only where the opponent deck is Rally."""
    labels = list(labelled_seeds)
    require(len(labels) == BLOCKS, 'Evaluation needs 32 seed blocks')
    jobs = []
    for j, label in enumerate(labels):
        for member in members:
            for i, deck in enumerate(DECKS):
                if member['scope'] == 'rally' and (i + j) % 7 != RALLY:
                    continue
                for seat in SEATS:
                    jobs.append(dict(id=f"{prefix}-b{j:02d}-{member['id']}-d{i}-s{seat}", block=j, seed_label=label,
                                     seed=labelled_seeds[label], member=member['id'], learner_deck=deck,
                                     opponent_deck=opponent_deck(i, j), learner_seat=seat, rotation=j % 7))
    return jobs


def expected_bo3(members):
    return sum(PER_RALLY_MEMBER if m['scope'] == 'rally' else PER_FULL_MEMBER for m in members)


def native_match(job, packet):
    """The public_feature_evaluation_v1 match for one job; the learner sits at its seat index."""
    by_seat = [job['learner_deck'], job['opponent_deck']] if job['learner_seat'] == 0 else \
        [job['opponent_deck'], job['learner_deck']]
    config = dict(deck_ids=by_seat, seed=job['seed'], game_one_chooser=GAME_ONE_CHOOSER, **MATCH_OPTIONS)
    return dict(config=config, registered=[packet['decks'][deck] for deck in by_seat],
                postboard=[packet['postboard'][by_seat[0] + '|' + by_seat[1]],
                           packet['postboard'][by_seat[1] + '|' + by_seat[0]]])


RECENT_BINDINGS_SCHEMA = 'g115-line-a-recent-bindings/v1'


def draft_roster(g115_source, panel_imports=None, recent_bindings=None):
    """The ruling's roster as a DRAFT (not the frozen declaration): order, weights and exposure stay Codex's."""
    def pending(reason):
        return dict(status='pending', reason=reason)

    imports = {m['label']: m for m in (panel_imports or {}).get('members', [])}
    members = [
        dict(id='v3', role='v3', scope='full', lineage='v3',
             evaluation=dict(status='bound', source=dict(kind='legacy', source=V3_SOURCE, v3_forced_actions=True,
                                                         v3_spell_target_reference_adapter=True)),
             training=pending(NOT_TRAINABLE['v3'])),
        dict(id='d3-wrapper', role='d3-wrapper', scope='full', lineage='g115 (search wrapper)',
             evaluation=dict(status='bound', source=dict(kind='information_set_search_v3', source=g115_source,
                                                         descriptor=SEARCH_DESCRIPTOR)),
             training=pending(NOT_TRAINABLE['d3-wrapper']))]
    bindings = {}
    if recent_bindings is not None:
        require(recent_bindings['schema'] == RECENT_BINDINGS_SCHEMA, 'Wrong recent bindings schema')
        bindings = {m['id']: m for m in recent_bindings['members']}
    for member_id, (replica, arm, checkpoint) in RECENT_ENDPOINTS.items():
        root = 'E:/mtg-postboard-campaign-20260921/control-variance-002/%s' % replica
        evaluation = dict(status='pending', checkpoint=dict(
            path='%s/endpoints/%s/checkpoint.json' % (root, arm), sha256=checkpoint),
            reason='public_checkpoint source needs its run config pin (CODEX #529 packet not supplied)')
        binding = bindings.get(member_id)
        if binding is not None:
            source = binding['evaluation_source']
            require(source['kind'] == 'public_checkpoint' and source['checkpoint']['sha256'] == checkpoint,
                    'Recent binding differs from the ruling endpoint: ' + member_id)
            # Raw config-file and checkpoint semantic config hashes are different domains (CODEX #529).
            evaluation = dict(status='bound', source=source, optimizer=binding['optimizer'],
                              semantic_config_sha256=binding['checkpoint_semantic_config_sha256'])
        members.append(dict(id=member_id, role='recent', scope='full',
                            lineage='g115 continuation %s/%s' % (replica, arm), evaluation=evaluation,
                            training=pending(NOT_TRAINABLE['recent'])))
    for role, slots in (('population', POPULATION_SLOTS), ('august', AUGUST_SLOTS)):
        for slot in slots:
            imported = imports.get('refresh-034/' + slot)
            status = imported['status'] if imported else 'no panel import manifest supplied'
            evaluation = pending('panel import: ' + status)
            if imported and imported.get('status') == 'playable' and imported.get('model_source'):
                # CODEX #520 item 5: true/true authority adapters for explicitly V3-transferred members.
                evaluation = dict(status='bound', source=dict(kind='legacy', source=imported['model_source'],
                                                              v3_forced_actions=True,
                                                              v3_spell_target_reference_adapter=True))
            members.append(dict(id='%s/%s' % (role, slot), role=role, scope='rally',
                                lineage=ARCHIVAL_LINEAGE[slot], evaluation=evaluation,
                                training=pending(NOT_TRAINABLE[role])))
    # R7: version 1 has no holdout; b/block48 is a declared g115 ancestor (CODEX #530).
    roles = {role: [m['id'] for m in members if m['role'] == role] for role in ROLES}
    return dict(schema=ROSTER_SCHEMA, frozen=False, version=1, template=TEMPLATE,
                basis='DIRECTOR-RULINGS-20260927.md R2, R3, R7 (draft order; the declaration fixes order, '
                      'lineages, exposure and weights)', roles=roles, members=members, exposure_tables={},
                yardstick_weights=None)


def validate_exposure(table, roster, template, composition):
    """Structural checks only: the declaration chooses the table; this refuses tables that break the ruling."""
    require(table['schema'] == EXPOSURE_SCHEMA and table['composition'] == composition, 'Wrong exposure table')
    episodes = check_template_shape(template)
    assignments = table['assignments']
    require(len(assignments) == len(episodes), 'Exposure table must assign all 2,000 episodes')
    allowed = {m['id']: m for m in composition_members(roster, composition)}
    for index, (member_id, episode) in enumerate(zip(assignments, episodes)):
        require(member_id in allowed, 'Episode %d assigned outside the composition: %s' % (index, member_id))
        rally_slot = opponent_label(episode) == 'Rally'
        archival = allowed[member_id]['role'] in ARCHIVAL_ROLES
        require(archival == (rally_slot and composition == 'E'), 'Episode %d breaks the Rally-slot rule (R3)' % index)
    counts = {}
    for member_id in assignments:
        counts[member_id] = counts.get(member_id, 0) + 1
    return counts


def training_config(template, schedule, block, arm, opponents=None):
    """Template copy: only episode seed, id and (treatment) opponent fields change.

    `opponents[e]` holds flattened episode e's per-episode opponent fields, for example
    {'opponent': ...} or {'opponent': ..., 'opponent_search': ...}.
    """
    config = copy.deepcopy(template)
    for index, episode in enumerate(check_template_shape(config)):
        update, slot = divmod(index, seeds.GAMES_PER_UPDATE)
        episode['seed'] = schedule['seeds'][index]
        episode['id'] = f'la-b{block:02d}-i{update:03d}-s{slot:02d}'
        if arm == 'treatment':
            episode.pop('opponent', None)
            episode.update(copy.deepcopy(opponents[index]))
    return config


def config_bytes(config):
    return json.dumps(config, separators=(',', ':'), ensure_ascii=True, allow_nan=False).encode('ascii')


def control_opponent_counts(template):
    counts = {}
    for episode in check_template_shape(template):
        key = episode['opponent']['checkpoint']['sha256']
        counts[key] = counts.get(key, 0) + 1
    return counts


def byte_worksheet(cap, measured=None):
    if measured is None:
        return dict(cap_bytes=cap, projected_bytes=None, status='unmeasured: qualification pending')
    return dict(cap_bytes=cap, projected_bytes=measured, status='measured')


def blocking(roster, slots, use, worksheet):
    reasons = [] if roster['frozen'] else ['roster is a draft; the frozen declaration fixes order, exposure and weights']
    reasons += ['member %s %s pending: %s' % (m['id'], use, m[use]['reason']) for m in slots
                if m[use]['status'] != 'bound']
    if worksheet['status'] != 'measured':
        reasons.append('byte worksheet ' + worksheet['status'])
    reasons.append('throughput evidence absent for the selected placement')
    reasons.extend('pending: ' + item for item in PENDING_CONTRACTS)
    return reasons


def provenance(roster, panel_imports_sha256):
    here = Path(__file__).resolve().parent
    return dict(builder=dict(path='python/tools/g115_line_a_manifest_v1.py',
                             sha256=file_sha256(here / 'g115_line_a_manifest_v1.py')),
                seed_consumer=dict(path='python/tools/g115_line_a_seeds_v1.py',
                                   sha256=file_sha256(here / 'g115_line_a_seeds_v1.py')),
                roster_sha256=canonical_sha256(roster), panel_imports_sha256=panel_imports_sha256)


def finish(manifest, roster, panel_imports_sha256):
    manifest['provenance'] = provenance(roster, panel_imports_sha256)
    manifest['manifest_sha256'] = canonical_sha256(manifest)
    return manifest


def seed_manifest_ref():
    return dict(path=seeds.SEED_MANIFEST_PATH, sha256=seeds.SEED_MANIFEST_SHA256)


def member_rows(members):
    return [dict(id=m['id'], role=m['role'], scope=m['scope']) for m in members]


def calibration_manifest(seed_manifest, packet, roster, composition, learner_source, imports_sha256=None):
    labelled = seeds.calibration_seeds(seed_manifest)
    members = composition_members(roster, composition)
    jobs = evaluation_jobs('cal', labelled, members)
    require(len(jobs) == expected_bo3(members), 'Calibration count differs')
    worksheet = byte_worksheet(CAPS['calibration'])
    reasons = blocking(roster, members, 'evaluation', worksheet)
    return finish(dict(
        schema=SCHEMAS['calibration'], composition=composition, dry_run=True, launchable=not reasons,
        blocking=reasons, seed_manifest=seed_manifest_ref(), decks=list(DECKS), deck_map_rule=DECK_MAP_RULE,
        deck_packet_sha256=canonical_sha256(packet), learner=dict(role='untouched g115', source=learner_source),
        members=member_rows(members), seeds=[dict(label=label, seed=value) for label, value in labelled.items()],
        match_options=dict(MATCH_OPTIONS, game_one_chooser=GAME_ONE_CHOOSER),
        yardstick_weights=roster['yardstick_weights'],
        counts=dict(bo3=len(jobs), full_members=sum(m['scope'] == 'full' for m in members),
                    rally_members=sum(m['scope'] == 'rally' for m in members), blocks=BLOCKS, seats=len(SEATS)),
        job_list_sha256=canonical_sha256(jobs), byte_budget=worksheet,
        scratch_root='D:/e-scratch/g115-line-a-calibration/',
        order='calibration is dispatched before any screen-training manifest (R10)'), roster, imports_sha256)


def screen_training_manifest(seed_manifest, template, roster, composition, imports_sha256=None):
    schedules = seeds.schedules(seed_manifest)
    require(control_opponent_counts(template) == {G115_CHECKPOINT_SHA256: 1000, A48_CHECKPOINT_SHA256: 1000},
            'Control template is not 1,000 g115 and 1,000 A48')
    members = composition_members(roster, composition)
    table_ref = roster['exposure_tables'].get(composition)
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
                run.update(opponents=dict(rule='R4 exposure table of the frozen declaration', table=table_ref),
                           config_sha256=None)
            runs.append(run)
    games = sum(run['episodes'] for run in runs)
    worksheet = byte_worksheet(CAPS['screen'])
    reasons = blocking(roster, members, 'training', worksheet)
    if table_ref is None:
        reasons.insert(0, 'exposure table for composition %s pending (declaration, R4)' % composition)
    return finish(dict(
        schema=SCHEMAS['screen-training'], composition=composition, dry_run=True, launchable=not reasons,
        blocking=reasons, seed_manifest=seed_manifest_ref(), template=TEMPLATE, schedule_rule=seeds.SCHEDULE_RULE,
        changed_fields=['updates[*][*].seed', 'updates[*][*].id', 'treatment only: updates[*][*] opponent fields'],
        loading_receipt=('the pre-update model, Adam moments and age traced to the pinned g115 input, and equal '
                         'across the matched arms of a block (CODEX #523 C2)'),
        members=member_rows(members), runs=runs, counts=dict(runs=len(runs), games=games),
        seeds=[dict(label=label, seed=schedules[label]['block_seed']) for label in seeds.TRAINING_LABELS],
        byte_budget=worksheet, scratch_root='D:/e-scratch/g115-line-a-screen/',
        order='refused unless the calibration completed and the frozen usability rule passed (R10)'),
        roster, imports_sha256)


def screen_evaluation_manifest(seed_manifest, packet, roster, composition, learner_source, holdout=False,
                               imports_sha256=None):
    labelled = seeds.screen_evaluation_seeds(seed_manifest)
    members = composition_members(roster, composition, holdout)
    per_endpoint = evaluation_jobs('scr', labelled, members)
    require(len(per_endpoint) == expected_bo3(members), 'Screen evaluation count differs')
    endpoints = [dict(id='g115', role='untouched g115, evaluated once and shared',
                      evaluation=dict(status='bound', source=learner_source))]
    for block in range(2):
        for arm in ('treatment', 'control'):
            endpoints.append(dict(id=f'la-screen-b{block:02d}-{arm}', role='trained endpoint',
                                  evaluation=dict(status='pending', reason='produced by the screen training run')))
    total = len(per_endpoint) * len(endpoints)
    worksheet = byte_worksheet(CAPS['screen'])
    reasons = blocking(roster, members + endpoints, 'evaluation', worksheet)
    return finish(dict(
        schema=SCHEMAS['screen-evaluation'], composition=composition, holdout=holdout, dry_run=True,
        launchable=not reasons, blocking=reasons, seed_manifest=seed_manifest_ref(),
        seed_rule='unsigned big-endian first 8 bytes of SHA256(ASCII("g115-line-a-seed-v2|" + label))',
        decks=list(DECKS), deck_map_rule=DECK_MAP_RULE, deck_packet_sha256=canonical_sha256(packet),
        members=member_rows(members),
        endpoints=[dict(id=e['id'], role=e['role'], status=e['evaluation']['status']) for e in endpoints],
        seeds=[dict(label=label, seed=value) for label, value in labelled.items()],
        match_options=dict(MATCH_OPTIONS, game_one_chooser=GAME_ONE_CHOOSER),
        yardstick_weights=roster['yardstick_weights'],
        counts=dict(bo3=total, endpoints=len(endpoints), per_endpoint=len(per_endpoint)),
        job_list_sha256=canonical_sha256(per_endpoint), byte_budget=worksheet,
        scratch_root='D:/e-scratch/g115-line-a-screen/'), roster, imports_sha256)


def build(seed_manifest, template, roster, imports_sha256=None):
    require(template['source']['checkpoint']['sha256'] == G115_CHECKPOINT_SHA256, 'Template does not start from g115')
    validate_roster(roster)
    packet = deck_packet(template)
    learner = dict(kind='legacy', source=template['source'], v3_forced_actions=False)
    result = dict(deck_packet=packet, roster=roster)
    for composition in COMPOSITIONS:
        suffix = '_' + composition
        result['calibration' + suffix] = calibration_manifest(seed_manifest, packet, roster, composition, learner,
                                                              imports_sha256)
        result['screen_training' + suffix] = screen_training_manifest(seed_manifest, template, roster, composition,
                                                                      imports_sha256)
        holdouts = (False, True) if any(m['role'] == 'holdout' for m in roster['members']) else (False,)
        for holdout in holdouts:
            name = 'screen_evaluation' + suffix + ('_holdout' if holdout else '')
            result[name] = screen_evaluation_manifest(seed_manifest, packet, roster, composition, learner, holdout,
                                                      imports_sha256)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--seed-manifest', type=Path, default=Path(seeds.SEED_MANIFEST_PATH))
    parser.add_argument('--template', type=Path, default=Path(TEMPLATE['path']))
    parser.add_argument('--roster', type=Path, help='Frozen g115-line-a-roster/v1 manifest; default: ruling draft')
    parser.add_argument('--panel-imports', type=Path, help='mtg-kernel-line-a-panel-imports/v1 manifest')
    parser.add_argument('--recent-bindings', type=Path, help='g115-line-a-recent-bindings/v1 packet (CODEX #529)')
    parser.add_argument('--out', type=Path, help='Fresh directory for the dry-run manifests')
    args = parser.parse_args()
    template = load_template(args.template)
    imports = imports_sha256 = None
    if args.panel_imports:
        imports = json.loads(args.panel_imports.read_bytes())
        imports_sha256 = file_sha256(args.panel_imports)
        require(imports['schema'] == 'mtg-kernel-line-a-panel-imports/v1', 'Wrong panel import schema')
    bindings = json.loads(args.recent_bindings.read_bytes()) if args.recent_bindings else None
    roster = json.loads(args.roster.read_bytes()) if args.roster else draft_roster(template['source'], imports,
                                                                                    bindings)
    result = build(seeds.load_seed_manifest(args.seed_manifest), template, roster, imports_sha256)
    if args.out:
        args.out.mkdir(parents=True)
        for name, value in result.items():
            (args.out / (name + '.json')).write_bytes(seeds.canonical_bytes(value))
    summary = {name: dict(sha256=value.get('manifest_sha256') or canonical_sha256(value), counts=value.get('counts'),
                          launchable=value.get('launchable'), blocking=len(value.get('blocking', [])))
               for name, value in result.items()}
    summary['panel_imports_sha256'] = imports_sha256
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
