"""Line (a) seed labels and the versioned block-seed episode schedule.

Engineering only: this module derives values and never selects them. The
frozen v2 manifest (schema g115-line-a-seeds/v2) is read by hash and every
stored label/value pair is rechecked against its rule before use.

The screen-training pair stores one u64 seed per training block. The episode
schedule expands that block seed into the 2,000 per-episode environment seeds
of one run with the kernel SplitMix64 (mtg-kernel/src/state.rs), the same
construction the BO3 evaluator applies to its run seed
(mtg-kernel/src/learned_bo3_v1.rs, SplitMix64::seed(config.seed) then one
next_u64 per game). The historical control-variance schedules used a
little-endian SHA256 label derivation; it is deliberately not reused here.
"""
import argparse
import hashlib
import json
from pathlib import Path

SEED_MANIFEST_SCHEMA = 'g115-line-a-seeds/v2'
SEED_MANIFEST_SHA256 = 'e3b010587a8cfc268b7163eabbc77b5777a978f3a71e3f69857be6a824bb89bb'
SEED_MANIFEST_PATH = 'E:/mtg-g115-lineage-20260923/line-a-proposal-001/seed-manifest.json'
LABEL_PREFIX = 'g115-line-a-seed-v2|'
CALIBRATION_LABELS = tuple(f'calibration/block/{j:02d}' for j in range(32))
TRAINING_LABELS = tuple(f'screen-training-pair/block/{b:02d}' for b in range(2))
SCREEN_EVALUATION_LABELS = tuple(f'screen-evaluation/block/{j:02d}' for j in range(32))

SCHEDULE_SCHEMA = 'g115-line-a-episode-schedule/v1'
SCHEDULE_RULE = ('episode e = games_per_update * update + slot receives the (e+1)-th next_u64 of the '
                 'kernel SplitMix64 (mtg-kernel/src/state.rs) seeded with the frozen block seed; no redraws')
UPDATES = 200
GAMES_PER_UPDATE = 10
EPISODES = UPDATES * GAMES_PER_UPDATE

_MASK = (1 << 64) - 1
_GAMMA = 0x9E3779B97F4A7C15


def require(ok, message):
    if not ok:
        raise ValueError(message)


def label_seed(label):
    """Unsigned big-endian first eight bytes of SHA256(ASCII(prefix + label))."""
    return int.from_bytes(hashlib.sha256((LABEL_PREFIX + label).encode('ascii')).digest()[:8], 'big')


def splitmix64(seed, count):
    """The first `count` outputs of the kernel SplitMix64 seeded with `seed`."""
    require(isinstance(seed, int) and not isinstance(seed, bool) and 0 <= seed <= _MASK, 'Seed must be a u64')
    state, out = seed, []
    for _ in range(count):
        state = (state + _GAMMA) & _MASK
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & _MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & _MASK
        out.append(z ^ (z >> 31))
    return out


def canonical_bytes(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True,
                      allow_nan=False).encode('ascii')


def canonical_sha256(value):
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def load_seed_manifest(path=SEED_MANIFEST_PATH):
    """Frozen manifest, hash-checked, with every label/value pair rederived."""
    raw = Path(path).read_bytes()
    require(hashlib.sha256(raw).hexdigest() == SEED_MANIFEST_SHA256, 'Seed manifest bytes differ from the frozen pin')
    manifest = json.loads(raw)
    require(manifest['schema'] == SEED_MANIFEST_SCHEMA, 'Wrong seed manifest schema')
    require(tuple(manifest['calibration_seed_labels']) == CALIBRATION_LABELS and
            tuple(manifest['matched_screen_training_block_labels']) == TRAINING_LABELS,
            'Seed labels differ from the declared order')
    pairs = (list(zip(manifest['calibration_seed_labels'], manifest['calibration_seed_blocks'])) +
             list(zip(manifest['matched_screen_training_block_labels'],
                      manifest['matched_screen_training_block_seeds'])))
    require(len(pairs) == 34 and all(label_seed(label) == value for label, value in pairs),
            'Stored seed differs from its label rule')
    require(len({value for _, value in pairs}) == 34, 'Stored seeds are not distinct')
    return manifest


def calibration_seeds(manifest):
    return dict(zip(manifest['calibration_seed_labels'], manifest['calibration_seed_blocks']))


def training_block_seeds(manifest):
    return dict(zip(manifest['matched_screen_training_block_labels'],
                    manifest['matched_screen_training_block_seeds']))


def screen_evaluation_seeds(manifest):
    """Fresh packet labels under the unchanged v2 rule, disjoint from the manifest."""
    seeds = {label: label_seed(label) for label in SCREEN_EVALUATION_LABELS}
    frozen = set(calibration_seeds(manifest).values()) | set(training_block_seeds(manifest).values())
    require(len(set(seeds.values())) == 32 and not frozen & set(seeds.values()),
            'Screen-evaluation seeds collide')
    return seeds


def episode_schedule(manifest, block_label):
    """Versioned per-episode schedule of one frozen screen-training block."""
    blocks = training_block_seeds(manifest)
    require(block_label in blocks, 'Unknown screen-training block: ' + block_label)
    seeds = splitmix64(blocks[block_label], EPISODES)
    require(len(set(seeds)) == EPISODES, 'Episode seeds repeat within the block')
    return dict(schema=SCHEDULE_SCHEMA, rule=SCHEDULE_RULE, seed_manifest_sha256=SEED_MANIFEST_SHA256,
                block_label=block_label, block_seed=blocks[block_label], updates=UPDATES,
                games_per_update=GAMES_PER_UPDATE, seeds=seeds)


def schedules(manifest):
    """Both block schedules, refusing any seed shared across blocks or with the manifest."""
    result = {label: episode_schedule(manifest, label) for label in TRAINING_LABELS}
    first, second = (set(result[label]['seeds']) for label in TRAINING_LABELS)
    require(not first & second, 'Episode seeds repeat across training blocks')
    frozen = set(calibration_seeds(manifest).values()) | set(training_block_seeds(manifest).values())
    require(not (first | second) & frozen, 'Episode seeds repeat a frozen block seed')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--seed-manifest', type=Path, default=Path(SEED_MANIFEST_PATH))
    parser.add_argument('--write-schedules', type=Path,
                        help='Directory for the canonical schedule files (must not exist).')
    args = parser.parse_args()
    manifest = load_seed_manifest(args.seed_manifest)
    result = schedules(manifest)
    if args.write_schedules:
        args.write_schedules.mkdir(parents=True)
        for label, schedule in result.items():
            name = label.replace('/', '-') + '.schedule.json'
            (args.write_schedules / name).write_bytes(canonical_bytes(schedule))
    summary = dict(seed_manifest_sha256=SEED_MANIFEST_SHA256, schedule_schema=SCHEDULE_SCHEMA,
                   schedules={label: dict(block_seed=s['block_seed'], episodes=len(s['seeds']),
                                          schedule_sha256=canonical_sha256(s))
                              for label, s in result.items()},
                   screen_evaluation_seeds=screen_evaluation_seeds(manifest))
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
