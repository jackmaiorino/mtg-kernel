"""W5 chain controller (collab LANES/lead/W5-DESIGN.md v1.3 sections 2, 2.1, 2.2, 6, 8 and 9; amendment A1).

Plans, runs and supervises a node's W5 chains on top of the guarded launcher (`w5box.py`, E10) and the retention,
budget and selection rules (`retention.py`, E7). Stage 0 qualification chains, Stage 1a (C and X8 pairs) and
Stage 1b (XS, paired with Stage 1a's C chains, beside the frozen filler) all go through it.

  chains.py plan --inputs INPUTS.json        arm files, w5box launch plans, `w5-chain/v1` manifests (idempotent)
  chains.py launch --plan PLAN.json          box outages that hold the stage's first chains until every box is full
  chains.py run --plan PLAN.json --chain NAME [--qualification LABEL --max-wall-seconds S] [--accept-lower-bound R]
  chains.py supervise --plan PLAN.json [--loop SECONDS]
  chains.py status --plan PLAN.json
  chains.py end --plan PLAN.json --chain NAME --reason TEXT [--boundary B]
  chains.py segcheck --plan PLAN.json --arm ARM --out RECEIPT.json
  chains.py clean-fillers --plan PLAN.json [--apply]
  chains.py throughput --plan PLAN.json --arm ARM --label L --updates N --out RECORD.json [--counts 1,6]
  chains.py stage0-exit --record RECORD.json [--record ...] [--r-nom-usec R] --out REPORT.json
  internal: `_drive` and `_filler` (started by w5box inside a box), `_replay` (started by supervise)

**Inputs** (`w5-chains-inputs/v1`, written by the lead at freeze): `stage` (0, 1a, 1b), `label` (default
stage<stage>), `node`, `root` (the node's W5 root, one volume), `cgroup_root`, `isolation`, `host_class`, `layout`
(`w5box.py layout --out`), `python` (the interpreter w5box pins as the chain binary), `trainer`
(native_expanded_training_run_v1), `container_tool` (checkpoint_container_v1, weights snapshots), `base` (the prepared
30/50/20 opponent-schedule base), `frozen` (the section 8 freeze record, required in Stage 1), optional `byte_cap`,
`x_cap_seconds`, `r_nom_usec`, `c_wall_seconds` (Stage 0 only; Stage 1 is the frozen 7 days), and
- `arms`: per arm (C, X8, XS, and always F, the frozen filler): `parent` (dose record: the source the chain starts
  from), `workers`, `memory_max_bytes`, `bytes_per_update`, optional `segment_updates`, `base` and `throughput`
  (a `w5-box-throughput/v1` record), and for X arms `budget`, `coefficient`, `dose`;
- `chains`: name to `{arm, slot, seed_root, pair}` (F: a filler box, no pair, run by supervise through `w5box.py
  fill`, not by `run`; XS in Stage 1b: `partner`, the Stage 1a C
  chain in the same root; Stage 0 chains may omit the pair, e.g. a box throughput run with every box on one
  seed root, since a `w5-box-throughput/v1` record binds the arm file and needs identical work per box).
Stage 1 rules checked: seed roots `w5/chain<i>` (pair i) and `w5/filler<slot>`; 1a pairs C with X8 and 1b XS with
F, every box used, half per arm; F starts from C's parent; X chain i's configs share every iteration (episode ids,
deal seeds, seats, opponents) with C chain i's and differ only in the search block and resource keys (XS: the same
iterations; its parent and S recipe keys are reported).

**Pins.** w5box pins the interpreter (argv[0]) and the arm file (the "config"), so a box throughput record binds
an arm, not a chain. The arm file (content-addressed, `plan/arms/<ARM>-<sha12>.json`) pins the trainer, base,
parent, freeze record and every Python file the in-box driver loads; the driver refuses at start on any change, and
checks the trainer before each segment. `run` checks every plan pin before it calls w5box.

**Segments.** As EX1 Stage F (tools/ex1/stagef.py): native resume revalidates every completed iteration of a run,
so a chain is consecutive native runs, and bulk leaves only finished ones. Segment s covers global updates
[s L, (s + 1) L): `xact.build_config` at offset s L (episode ids, seeds and the placement key from the seed root and
the global update), its initial source the previous segment's last source with its last output checkpoint. The
driver writes the config, appends `{config, offset, root, updates}` to the manifest (atomic rewrite), and runs it.
A chain has no planned length: C stops at its wall, X at R_i (or the cap); a filler runs until it is stopped.

**In-box driver** (`_drive`, the chain process w5box meters; its own CPU, a few configs and hashes per segment,
is part of the chain's ledger in every arm). Per segment: END, STOP (`<root>/STOP` or `chains/<name>/STOP`: stop
at the next segment boundary; `w5box.py kill` stops at once) and byte guard admission (a fresh verdict whose
ceiling for this chain, fixed at supervise's census, is above the segment's index); an interrupted iteration
is moved aside (mtg-kernel#336). C: from `launched_unix` (the first start) plus the C wall, the trainer is killed
and the chain ends at its last completed boundary (END `c-wall`). X: a trainer that stops at the ledger source's
`stop_at_usec` (resume to R_i) ends the chain (END `x-stop-at`). Any other failure writes FAILURE.json and exits 8;
whether it is a chain failure (section 7.4) is the lead's call (`end`), except an OOM kill, which `run` records.
Every close of a chain's instance (END, failure, STOP, kill, a qualification wall cap) leaves its box an outage in
w5box's node log (the plan marks each chain spec `handoff: true`), so the node's other measured chains hold until
the chain restarts in its box or, once it has ended, a verified filler takes the box; a Stage 0 run that stops
without an END keeps its box held until it reruns or the lead ends it.

**Supervise** (per node, off the boxes; one at a time). Each cycle: index every measured chain; the 50% R_nom look
boundary (`r_nom_usec`, frozen in Stage 1) is recorded and protected from every prune; for each pair, before C ends,
`retention.decide` (projection over the C wall): the snapshot window (from the first boundary at 0.85 R-hat, cut
with the container tool, kept open until R_i is final or 30 GB of snapshots), pause (`w5box.py pause --now` once),
stop at the X cap (END `x-cap`, kill; enforced before X's first update too); once C ends (END) and its instance
closes, R_i = its final closed ledger (at its wall) or the ledger at its last valid boundary (an early end): a
paused X resumes with `{"stop_at_usec": R_i}`, a running X without that stop is paused and resumed with it, an X
past R_i ends (END `reached-r`, kill). When both chains have ended and no instance is open: endpoints (C: its last
valid boundary; X: the first valid boundary at or past R_i, else its last valid one, labelled under budget; a failed
or lead-ended X: its recorded last valid boundary, under budget), `retention.select`, a replay where the state is
gone (run detached by `_replay`, then `verify-replay`), the PRUNE manifest (look checkpoints kept), and the pruned
files moved to the same-volume hold folder `<root>/hold` with a move manifest, purged after `hold_days`. Then the
rolling retention of running chains (`retention.plan` / `apply`); kills retried for ended chains still running;
occupancy (a box whose measured chain has ended, and every filler box, holds the frozen filler while the stage runs,
through `w5box.py fill`, which stops once all measured chains have ended); the byte guard verdict
(`<root>/GUARD.json`).

**Implementation choices** (not design constants):
- Segment lengths: C 256, X8 and XS 16, F 64 updates (C's hot trajectories over 64 segments stay near 16k updates;
  X's segments are minutes long). Plan refuses 64 x L x bytes_per_update above 60 GB per chain (section 9).
- Rolling retention deletes directly: it removes only regenerable files (retention.py: checkpoints by replay
  against the recorded hashes, trajectories by recollection), at about 15 MB per C update, which no hold folder
  could keep. Post-selection PRUNE moves go through the hold folder (7 days).
- X running wall (the cap) = open w5box instances minus the chain's own pauses, from its ledger; node freezes and
  restarts' gaps are not subtracted or added respectively.
- C's wall is calendar time from its first start; at the deadline the in-flight update is discarded (C updates
  are about half a second).
- Cadences: driver trainer poll 1 s (C only), guard verdict valid 15 minutes, a driver waits up to 30 minutes for
  one (then exits 7); supervise default 60 s. One running plan per node (the guard counts its chains).
- Fillers (`_filler` in a paused box, an ended box or a filler box) start fresh from the C parent at update 0 and
  delete each segment once the next one finishes; their trees go with `clean-fillers` at the stage's end. A box
  left by an ended chain becomes an outage when supervise's next cycle starts `w5box.py fill` (`--empty-since`
  records the chain's close, so the unfrozen gap of at most one cycle is on record).
- Byte guard: each verdict reserves 5 segments per measured chain (one in flight, GUARD_SEGMENTS = 4 admitted)
  and two F segments per box (fillers do not wait on the guard; their hot set is bounded).

**Box throughput** (`throughput`, Stage 0, COMPUTE-POLICY items 2 to 5): writes the `w5-box-throughput/v1` record
w5box's guard requires for one arm of a plan. Each phase (default: one box, then every box of the layout) is a
Stage 0 plan derived from the plan's own inputs, so the arm file, filler, environment and w5box launch
specification are byte-identical and the phase binds what the plan launches (checked: equal throughput bindings).
Its chains (`<label>-<arm>-p<count>-s<slot>`, slots 0 to count - 1, all on one seed root, default
`w5/throughput`) run fresh through `chains.py run` in qualification mode, under this command's own supervise
cycles without fills; the node runs whole until every box has completed N updates (so each box's window has every
other box busy), then every chain ends at once (STOP files, then w5box kills: a chain ended alone would leave a
handoff outage freezing the rest), and a chain that exits before every box is done fails the phase.
Per box: updates N, the ledger at boundary N (in-box CPU), the in-box wall from the instance's start to boundary
N less node-frozen intervals (a phase's first chains start held until every box is occupied), and identity rows
(the after-state hash of every boundary 1 to N), which must be equal in every box of every phase. The record also
carries rates, in-box utilization (CPU per wall over the box's threads), each box's chain `memory.peak` and the
node's lowest MemAvailable and highest Committed_AS while the phase ran. A passing record goes to `--out` and the
qualification chains' segment trees (regenerable bulk) are removed; a failing one goes to `<out>.failed.json`
with every tree kept. `stage0-exit` reads records of several arms and reports, without deciding, the Stage 0 exit
numbers of section 6: per arm in-box utilization, R_nom (given, else provisional: the C record's mean full-node box
CPU rate x 7 days), each X arm's projected days to R_nom, and where the worst X arm falls in the declared
response (launch; cap ceil(1.2 x days); 3-core boxes; stop). Choices (not design constants): N is whole segments,
at least two (so per-segment startup is in the window); a projection uses the arm's mean full-node box rate (the
slowest box is reported beside it); a single-arm full node stands in for Stage 1's mixed node.
"""
import argparse
import copy
import json
import math
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent / 'ex1'))
import qual  # noqa: E402
import retention  # noqa: E402
import w5box  # noqa: E402
import xact  # noqa: E402

INPUTS_SCHEMA = 'w5-chains-inputs/v1'
PLAN_SCHEMA = 'w5-chains-plan/v1'
ARM_SCHEMA = 'w5-arm/v1'
END_SCHEMA = 'w5-chain-end/v1'
PAIR_SCHEMA = 'w5-pair/v1'
SEGCHECK_SCHEMA = 'w5-segcheck/v1'
ARMS = {'C': ('C', 'C'), 'X8': ('X', 'X'), 'XS': ('X', 'X'), 'F': ('C', None)}  # recipe, retention arm
MEASURED = ('C', 'X8', 'XS')
STAGES = ('0', '1a', '1b')
SEGMENT_UPDATES = {'C': 256, 'X8': 16, 'XS': 16, 'F': 64}
HOT_CAP_BYTES = 60 * 10**9
BYTE_CAP = 560 * 10**9
HOLD_DAYS = 7
C_WALL = retention.HORIZON
LOOK_FRACTION = 0.5
GUARD_MAX_AGE = 900.0
GUARD_WAIT = 1800.0
GUARD_POLL = 30.0
GUARD_SEGMENTS = 4
END_KILL_GRACE = 30.0
FILL_START_GRACE = 60.0
TRAINER_POLL = 1.0
SUPERVISE_EVERY = 60.0
REPLAY_START_GRACE = 120.0
EXIT_STOP, EXIT_ROOM, EXIT_FAILED = 5, 7, 8
FAILED_ENDS = ('chain-failure', 'operator')  # END reasons whose recorded last valid boundary is the endpoint
PAIR_ONLY_KEYS = {'ex1', 'preparation_workers', 'output_directory', 'chain_ledger', 'collection_workers'}
TOOL_FILES = {'chains.py': HERE / 'chains.py', 'retention.py': HERE / 'retention.py', 'xact.py': HERE / 'xact.py',
              'w5box.py': HERE / 'w5box.py', 'qual.py': HERE.parent / 'ex1' / 'qual.py',
              'target_ref.py': HERE.parent / 'ex1' / 'target_ref.py'}
NAME = re.compile(r'[a-z0-9][a-z0-9-]{0,31}')


class Stop(Exception):
    """A refusal or a stop the caller reports."""


def require(ok, message):
    if not ok:
        raise Stop(message)


# ---------------------------------------------------------------- files

def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write(path, doc):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f'{path.name}.{os.getpid()}.{time.time_ns()}.tmp')
    with open(tmp, 'w', encoding='utf-8', newline='\n') as f:
        f.write(json.dumps(doc, indent=1, sort_keys=True) + '\n')
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def write_once(path, doc):
    """Creates `path` only if it does not exist (a hard link of a complete temporary file); False if it did."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f'{path.name}.{os.getpid()}.{time.time_ns()}.tmp')
    tmp.write_text(json.dumps(doc, indent=1, sort_keys=True) + '\n', encoding='utf-8', newline='\n')
    try:
        os.link(tmp, path)
    except FileExistsError:
        return False
    finally:
        tmp.unlink()
    return True


def sha(path):
    return retention.sha(path)


def pin(path):
    return {'path': Path(path).as_posix(), 'sha256': sha(path)}


def check_pin(p, what):
    require(p is not None and Path(p['path']).exists() and sha(p['path']) == p['sha256'],
            f'{what} differs from its pin ({None if p is None else p["path"]})')


def log(root, **row):
    row['unix'] = round(time.time(), 3)
    path = Path(root) / 'LOG.jsonl'
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, 'a', encoding='utf-8', newline='\n') as f:
        f.write(json.dumps(row, sort_keys=True) + '\n')


def exe(path):
    return [sys.executable, path] if str(path).endswith('.py') else [str(path)]  # .py: test stand-ins only


def lock(path):
    """An exclusive OS lock held by the returned handle (as tools/ex1/stagef.py); None if another holder has it."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    f = open(path, 'a+b')
    try:
        if os.name == 'nt':
            import msvcrt
            f.seek(0)
            msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(f.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        f.close()
        return None
    return f


def unlock(f):
    try:
        if os.name == 'nt':
            import msvcrt
            f.seek(0)
            msvcrt.locking(f.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(f.fileno(), fcntl.LOCK_UN)
    finally:
        f.close()


def is_link(path):
    p = Path(path)
    return p.is_symlink() or (hasattr(p, 'is_junction') and p.is_junction())


def tools_shas():
    return {name: sha(path) for name, path in TOOL_FILES.items()}


# ---------------------------------------------------------------- segments (as tools/ex1/stagef.py)

def completed(native):
    return len(list(Path(native, 'iterations').glob('*/complete.json')))


def attempt_dir(iteration, complete):
    return iteration / [p for p in re.split(r'[\\/]', complete['update']['path']) if p.startswith('attempt-')][0]


def next_source(native):
    """The source the update after this run's last one reads: its last source with the checkpoint replaced by its
    last output checkpoint (checked against the pinned hash)."""
    its = sorted(p for p in (Path(native) / 'iterations').iterdir() if (p / 'complete.json').exists())
    require(its, f'{native} has no completed iteration')
    last = its[-1]
    complete = read(last / 'complete.json')
    checkpoint = attempt_dir(last, complete) / 'update' / 'checkpoint.json'
    digest = complete['output_identity']['checkpoint_sha256']
    require(checkpoint.exists() and sha(checkpoint) == digest, f'{checkpoint} is missing or differs from its pin')
    source = copy.deepcopy(complete['source'])
    source['checkpoint'] = {'path': checkpoint.as_posix(), 'sha256': digest}
    return source


def quarantine(native, dest_root):
    """mtg-kernel#336: an iteration without complete.json is moved aside, so the run reruns it fresh. Its bulk
    (trajectories, checkpoints, S containers, and any staging file of an unfinished publication) is regenerable and
    deleted first, so the attempt keeps only its records and moved-aside attempts never grow a chain's hot bytes."""
    moved = []
    iterations = Path(native) / 'iterations'
    if not iterations.exists():
        return moved
    for d in sorted(p for p in iterations.iterdir() if p.is_dir()):
        if not (d / 'complete.json').exists():
            for f in sorted(d.rglob('*')):
                base = f.name.lstrip('.')
                if f.is_file() and not f.is_symlink() and (base.endswith('.stage') or base.startswith('episode-')
                                                             or f.suffix == '.bin' or base == 'checkpoint.json'):
                    f.unlink()  # a staging file (`.<name>.stage`) is an unfinished publication: never a record
            dest = Path(dest_root) / 'interrupted' / f'{Path(native).name}-{d.name}-{time.time_ns()}'
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(d), str(dest))
            moved.append({'from': d.as_posix(), 'to': dest.as_posix()})
    return moved


def arm_config(arm, seed_root, offset, updates, native, source=None, ledger_source=None):
    """The arm's segment config: EX1's construction keyed by the seed root (xact.build_config), sliced at the
    global update `offset`; `source` replaces the parent for every segment after the first."""
    x = arm['x'] or {}
    cfg = xact.build_config(read(arm['base']['path']), read(arm['parent']['path']), native, seed_root, offset,
                            updates, arm['workers'], x.get('budget'), x.get('coefficient'), x.get('dose', 'primary'),
                            ledger_source)
    if source is not None:
        cfg['initial_source'] = source
    return cfg


def run_trainer(arm, config_path, deadline=None, logs=None):
    """Runs one native segment in its own process group; at `deadline` (C's wall) the group is killed. With
    `logs` (a directory) the segment's stdout replaces `trainer.out` and stderr is appended to `trainer.err`."""
    check_pin(arm['trainer'], 'the trainer')
    kwargs = {'creationflags': subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == 'nt' else \
        {'start_new_session': True}
    if logs:
        Path(logs).mkdir(parents=True, exist_ok=True)
    out = open(Path(logs) / 'trainer.out', 'wb') if logs else None
    err = open(Path(logs) / 'trainer.err', 'ab') if logs else None
    try:
        proc = subprocess.Popen(exe(arm['trainer']['path']) + [str(config_path)], stdout=out, stderr=err,
                                stdin=subprocess.DEVNULL, **kwargs)
        if deadline is None:
            return proc.wait(), False
        while True:
            try:
                return proc.wait(timeout=TRAINER_POLL), False
            except subprocess.TimeoutExpired:
                if time.time() >= deadline:
                    if os.name == 'nt':
                        proc.kill()
                    else:
                        try:
                            os.killpg(proc.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    return proc.wait(), True
    finally:
        for f in (out, err):
            if f:
                f.close()


# ---------------------------------------------------------------- plan

def history_relative(base):
    return sorted({e['opponent'].get('kind') for it in base['iterations'] for e in it['episodes']
                   if e.get('opponent', {}).get('kind') in retention.HISTORY_KINDS})


def recipe_identity(arm):
    """What a chain's segments depend on; a manifest with segments refuses a plan that changes it."""
    return {k: arm[k] for k in ('arm', 'recipe', 'base', 'parent', 'x', 'workers', 'segment_updates',
                                'c_wall_seconds')} | {'trainer_sha256': arm['trainer']['sha256']}


def pair_difference(a, b, allowed=PAIR_ONLY_KEYS):
    return sorted(k for k in set(a) | set(b) if k not in allowed and a.get(k) != b.get(k))


def check_pair(x_arm, c_arm, seed_root, scratch):
    """X chain i and C chain i: the same repeating schedule (the bases' iterations and opponent rosters, which with
    the seed root and the global update fix every episode id, deal seed, seat and opponent), and identical
    generated iterations at two offsets; X8 differs from C only in the search block and resource keys. Returns
    XS's other differences (reported)."""
    x_base, c_base = read(x_arm['base']['path']), read(c_arm['base']['path'])
    require(x_base['iterations'] == c_base['iterations'] and x_base.get('opponents') == c_base.get('opponents'),
            f'{x_arm["arm"]} and C use different opponent schedules for {seed_root}')
    other = set()
    for offset in (0, 1_000_003):
        x_cfg = arm_config(x_arm, seed_root, offset, 2, scratch / 'x')
        c_cfg = arm_config(c_arm, seed_root, offset, 2, scratch / 'c')
        require(x_cfg['iterations'] == c_cfg['iterations'] and x_cfg.get('opponents') == c_cfg.get('opponents'),
                f'{x_arm["arm"]} and C differ in iterations or opponents for {seed_root}')
        diff = pair_difference(x_cfg, c_cfg)
        if x_arm['arm'] == 'X8':
            require(not diff, f'X8 and C differ outside the search block for {seed_root}: {diff}')
        other |= set(diff)
    return sorted(other)


def make_plan(inputs_path):
    inputs = read(inputs_path)
    require(inputs.get('schema') == INPUTS_SCHEMA, 'not a w5-chains-inputs/v1 document')
    stage = inputs['stage']
    require(stage in STAGES, f'stage must be one of {STAGES}')
    label = inputs.get('label', f'stage{stage}')
    require(NAME.fullmatch(label) is not None, 'label must be lowercase letters, digits and dashes')
    root = Path(inputs['root'])
    require(root.is_absolute(), 'root must be absolute')
    layout_doc = read(inputs['layout'])
    layout = {k: layout_doc[k] for k in w5box.LAYOUT_KEYS}
    require(layout_doc.get('host_class', inputs['host_class']) == inputs['host_class'],
            'the layout was detected on another host class')
    boxes = len(layout['boxes'])
    frozen = inputs.get('frozen')
    require(stage == '0' or frozen, 'Stage 1 needs the frozen-constants record')
    c_wall = float(inputs.get('c_wall_seconds', C_WALL))
    require(c_wall > 0 and (stage == '0' or c_wall == C_WALL), 'the C wall is the frozen 7 days outside Stage 0')
    x_cap = float(inputs.get('x_cap_seconds', retention.X_CAP_SECONDS))
    require(x_cap > 0, 'x_cap_seconds must be positive')
    r_nom = inputs.get('r_nom_usec')
    require(r_nom is None or (type(r_nom) is int and r_nom > 0), 'r_nom_usec must be a positive integer')
    require(stage == '0' or r_nom is not None, 'Stage 1 needs the frozen R_nom (r_nom_usec)')
    byte_cap = int(inputs.get('byte_cap', BYTE_CAP))
    arms_in, chains_in = inputs['arms'], inputs['chains']
    used = {c['arm'] for c in chains_in.values()} | {'F'}
    for a in used:
        require(a in ARMS and a in arms_in, f'arm {a} is unknown or has no inputs')
    if stage == '1a':
        require({c['arm'] for c in chains_in.values()} == {'C', 'X8'}, 'Stage 1a runs C and X8 chains')
    if stage == '1b':
        require({c['arm'] for c in chains_in.values()} == {'XS', 'F'}, 'Stage 1b runs XS and F chains')
    tools = tools_shas()
    common = {k: pin(inputs[k]) for k in ('python', 'trainer', 'container_tool', 'base', 'layout')}
    frozen_pin = pin(frozen) if frozen else None
    arms, arm_paths = {}, {}
    for a in sorted(used):
        spec = arms_in[a]
        recipe, ret = ARMS[a]
        length = int(spec.get('segment_updates', SEGMENT_UPDATES[a]))
        require(1 <= length <= 4096, f'{a}: segment_updates must be 1 to 4096')
        bpu = int(spec['bytes_per_update'])
        require(bpu > 0 and int(spec['memory_max_bytes']) > 0 and int(spec['workers']) >= 1,
                f'{a}: bytes_per_update, memory_max_bytes and workers are required')
        # Between two prunes a chain holds its 64 kept segments, the one in flight at the census and the
        # GUARD_SEGMENTS its verdict admits.
        require(a == 'F' or (retention.TRAJECTORY_SEGMENTS + 1 + GUARD_SEGMENTS) * length * bpu <= HOT_CAP_BYTES,
                f'{a}: {retention.TRAJECTORY_SEGMENTS + 1 + GUARD_SEGMENTS} segments of {length} updates at {bpu} '
                'bytes pass the 60 GB per-chain hot cap')
        base = pin(spec.get('base', inputs['base']))
        require(not history_relative(read(base['path'])), f'{a}: the base uses history-relative opponents, which '
                'segments would change')
        x = None
        if recipe == 'X':
            require(type(spec.get('budget')) is int and spec['budget'] > 0 and spec.get('dose') in xact.DOSES
                    and isinstance(spec.get('coefficient'), (int, float)) and spec['coefficient'] > 0,
                    f'{a}: an X arm needs budget, coefficient and dose')
            x = {'budget': spec['budget'], 'coefficient': float(spec['coefficient']), 'dose': spec['dose']}
        else:
            require(not {'budget', 'coefficient', 'dose'} & set(spec), f'{a}: a C-recipe arm has no search block')
        doc = {'schema': ARM_SCHEMA, 'arm': a, 'recipe': recipe, 'retention_arm': ret, 'root': root.as_posix(),
               'trainer': common['trainer'], 'base': base, 'parent': pin(spec['parent']), 'frozen': frozen_pin,
               'x': x, 'workers': int(spec['workers']), 'segment_updates': length, 'bytes_per_update': bpu,
               'c_wall_seconds': c_wall if a == 'C' else None, 'tools': tools}
        text = json.dumps(doc, indent=1, sort_keys=True) + '\n'
        digest = w5box.sha256_bytes(text.encode('utf-8'))
        path = root / 'plan' / 'arms' / f'{a}-{digest[:12]}.json'
        if not path.exists():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding='utf-8', newline='\n')
        require(sha(path) == digest, f'{path} exists with other content')
        arms[a], arm_paths[a] = doc, path
    if 'C' in arms:
        require(arms['F']['parent']['sha256'] == arms['C']['parent']['sha256'], 'the filler starts from C\'s parent')
    if 'X8' in arms_in and 'XS' in arms_in:
        require(arms_in['X8'].get('budget') == arms_in['XS'].get('budget') and
                arms_in['X8'].get('dose') == arms_in['XS'].get('dose'), 'X8 and XS share B and the dose')
    # chains, slots, seed roots and pairs
    chains, slots, pairs, partners = {}, set(), {}, {}
    for name, c in sorted(chains_in.items()):
        require(NAME.fullmatch(name) is not None, f'chain name {name!r}: lowercase letters, digits and dashes')
        a, slot, seed_root = c['arm'], c['slot'], c['seed_root']
        require(type(slot) is int and 0 <= slot < boxes and slot not in slots, f'{name}: bad or repeated slot')
        slots.add(slot)
        if a == 'F':
            require('pair' not in c, f'{name}: a filler has no pair')
            require(stage == '0' or seed_root == f'w5/filler{slot}', f'{name}: filler seed root is w5/filler<slot>')
        elif c.get('pair') is None:  # Stage 0 only: e.g. a box throughput run, every box on one seed root
            require(stage == '0' and not c.get('partner'), f'{name}: a Stage 1 chain belongs to a pair')
        else:
            p = c['pair']
            require(type(p) is int and p >= 1, f'{name}: pair must be a positive integer')
            require(stage == '0' or seed_root == f'w5/chain{p}', f'{name}: Stage 1 seed root is w5/chain<pair>')
            role = 'C' if a == 'C' else 'X'
            require(role not in pairs.setdefault(p, {}), f'pair {p} has two {role} chains')
            pairs[p][role] = name
            if c.get('partner'):
                require(a == 'XS', f'{name}: only XS names a Stage 1a partner')
                partners[p] = c['partner']
        if stage == '0':
            require(not seed_root.startswith(('w5/chain', 'w5/filler')) and seed_root != 'w5/0b',
                    f'{name}: Stage 0 seed roots stay out of Stage 1 and 0b namespaces')
        chains[name] = {'arm': a, 'slot': slot, 'seed_root': seed_root, 'pair': c.get('pair'),
                        'manifest': None if a == 'F' else (root / 'chains' / name / 'chain.json').as_posix()}
    seeds = {}
    for name, c in chains.items():
        if c['pair'] is not None:
            seeds.setdefault(c['seed_root'], set()).add(c['pair'])
    require(all(len(v) == 1 for v in seeds.values()), 'two pairs share a seed root')
    if stage != '0':
        counts = {}
        for c in chains.values():
            counts[c['arm']] = counts.get(c['arm'], 0) + 1
        require(len(chains) == boxes and len(set(counts.values())) == 1,
                f'Stage {stage} uses every box, half per arm: {counts} on {boxes} boxes')
    scratch = root / 'plan' / label / '_paircheck'
    xs_other = {}
    plan_pairs = {}
    for p, members in sorted(pairs.items()):
        if 'C' not in members and p in partners:
            cname = partners[p]
            cman_path = root / 'chains' / cname / 'chain.json'
            require(cman_path.exists(), f'pair {p}: partner {cname} has no manifest in this root')
            cman = read(cman_path)
            require(cman['arm_name'] == 'C' and cman['seed_root'] == chains[members['X']]['seed_root'],
                    f'pair {p}: partner {cname} is not C chain {p}')
            require((cman_path.parent / 'END.json').exists(), f'pair {p}: partner {cname} has not ended')
            c_arm = read(cman['arm_file']['path'])
            require(arms['F']['parent']['sha256'] == c_arm['parent']['sha256'], 'the filler starts from C\'s parent')
            members['C'] = cname
            plan_pairs[str(p)] = {'C': cname, 'X': members['X'], 'c_external': True}
        else:
            require(stage == '0' or set(members) == {'C', 'X'}, f'pair {p} needs a C and an X chain')
            if set(members) != {'C', 'X'}:
                continue
            c_arm = arms['C']
            plan_pairs[str(p)] = {'C': members['C'], 'X': members['X'], 'c_external': False}
        x_arm = arms[chains[members['X']]['arm']]
        other = check_pair(x_arm, c_arm, chains[members['X']]['seed_root'], scratch)
        if other:
            xs_other[str(p)] = other
    if stage == '1b':
        require(all(chains[n]['arm'] == 'F' or str(chains[n]['pair']) in plan_pairs for n in chains),
                'every XS chain names its Stage 1a partner')
    shutil.rmtree(scratch, ignore_errors=True)
    # manifests
    for name, c in chains.items():
        if c['arm'] == 'F':
            continue
        arm = arms[c['arm']]
        header = {'schema': 'w5-chain/v1', 'chain': name, 'arm': arm['retention_arm'], 'arm_name': c['arm'],
                  'seed_root': c['seed_root'], 'pair': c['pair'], 'stage': stage, 'node': inputs['node'],
                  'recipe': recipe_identity(arm), 'frozen': frozen_pin, 'x_cap_seconds': x_cap,
                  'c_wall_seconds': c_wall, 'r_nom_usec': r_nom,
                  'snapshot_dir': (root / 'chains' / name / 'snapshots').as_posix()}
        path = Path(c['manifest'])
        if path.exists():
            have = read(path)
            for k in ('chain', 'arm', 'arm_name', 'seed_root', 'pair', 'stage', 'frozen', 'x_cap_seconds',
                      'c_wall_seconds', 'snapshot_dir'):
                require(have.get(k) == header[k], f'{name}: the manifest has another {k}')
            require(not have['segments'] or have['recipe'] == header['recipe'],
                    f'{name}: the recipe changed after the chain started')
            require(have.get('r_nom_usec') in (None, r_nom), f'{name}: R_nom is frozen in the manifest')
            have.update(recipe=header['recipe'], arm_file=pin(arm_paths[c['arm']]), r_nom_usec=r_nom)
            write(path, have)
        else:
            write(path, dict(header, arm_file=pin(arm_paths[c['arm']]), launched_unix=None, segments=[]))
    # w5box launch plans, one per arm (a throughput record binds one arm)
    plan_dir = root / 'plan' / label
    py = common['python']
    filler = {'argv': [py['path'], str(HERE / 'chains.py'), '_filler', '--arm-file', arm_paths['F'].as_posix(),
                       '--seed-root', '{seed_root}', '--slot', '{slot}', '--seq', '{seq}'],
              'binary': py, 'config': pin(arm_paths['F']), 'memory_max_bytes': int(arms_in['F']['memory_max_bytes']),
              'seed_root_template': 'w5/filler{j}'}
    w5plans = {}
    for a in sorted({c['arm'] for c in chains.values()} - {'F'}):  # filler boxes run through `w5box.py fill`
        spec = {'box_slot': None, 'arm': a, 'binary': py, 'config': pin(arm_paths[a]),
                'argv': [py['path'], str(HERE / 'chains.py'), '_drive', '--arm-file', arm_paths[a].as_posix(),
                         '--chain', '{chain}', '--seed-root', '{seed_root}', '--state', '{state}',
                         '--ledger-source', '{ledger_source}'],
                'memory_max_bytes': int(arms_in[a]['memory_max_bytes'])}
        doc = {'schema': w5box.PLAN_SCHEMA, 'host_class': inputs['host_class'], 'layout': layout,
               'cgroup_root': inputs['cgroup_root'], 'state_root': (root / 'state').as_posix(),
               'isolation': inputs.get('isolation', 'partition-root'), 'stage': stage,
               'env': {'PYTHONDONTWRITEBYTECODE': '1'}, 'filler': filler,
               'chains': {n: dict(spec, box_slot=c['slot'], seed_root=c['seed_root'], handoff=True)
                          for n, c in sorted(chains.items()) if c['arm'] == a}}
        if arms_in[a].get('throughput'):
            doc['throughput'] = pin(arms_in[a]['throughput'])
        path = plan_dir / f'w5box-{a}.json'
        write(path, doc)
        w5plans[a] = pin(path)
    plan = {'schema': PLAN_SCHEMA, 'label': label, 'stage': stage, 'node': inputs['node'], 'root': root.as_posix(),
            'inputs': pin(inputs_path), 'frozen': frozen_pin, 'host_class': inputs['host_class'],
            'cgroup_root': inputs['cgroup_root'], 'byte_cap': byte_cap,
            'hold_days': float(inputs.get('hold_days', HOLD_DAYS)),
            'x_cap_seconds': x_cap, 'c_wall_seconds': c_wall, 'r_nom_usec': r_nom, **common, 'tools': tools,
            'arms': {a: {'file': pin(arm_paths[a]), 'w5box_plan': w5plans.get(a),
                         'throughput': bool(arms_in[a].get('throughput')),
                         'segment_updates': arms[a]['segment_updates'],
                         'bytes_per_update': arms[a]['bytes_per_update']} for a in arms},
            'chains': chains, 'pairs': plan_pairs, 'xs_recipe_differences': xs_other}
    path = plan_dir / 'CHAINS-PLAN.json'
    write(path, plan)
    return path, plan


def load_plan(path, check=True):
    plan = read(path)
    require(plan.get('schema') == PLAN_SCHEMA, 'not a w5-chains-plan/v1 document')
    if check:
        for key in ('python', 'trainer', 'container_tool', 'base', 'layout'):
            check_pin(plan[key], key)
        if plan['frozen']:
            check_pin(plan['frozen'], 'the frozen-constants record')
        for a, spec in plan['arms'].items():
            check_pin(spec['file'], f'arm file {a}')
            if spec['w5box_plan']:
                check_pin(spec['w5box_plan'], f'w5box plan {a}')
        require(plan['tools'] == tools_shas(), 'the W5 tools changed since the plan was written; plan again')
    return plan


def load_arm(path):
    arm = read(path)
    require(arm.get('schema') == ARM_SCHEMA, 'not a w5-arm/v1 document')
    require(arm['tools'] == tools_shas(), 'the W5 tools differ from the arm file pins')
    for key in ('trainer', 'base', 'parent'):
        check_pin(arm[key], key)
    if arm['frozen']:
        check_pin(arm['frozen'], 'the frozen-constants record')
    return arm


# ---------------------------------------------------------------- chain state

def chain_dir(root, name):
    return Path(root) / 'chains' / name


def read_end(root, name):
    path = chain_dir(root, name) / 'END.json'
    return read(path) if path.exists() else None


def stopped(root, cdir):
    return (Path(root) / 'STOP').exists() or (Path(cdir) / 'STOP').exists()


def latest(man):
    """(latest boundary, its receipt or None) from the manifest's segments."""
    segs = man['segments']
    for seg in reversed(segs):
        n = completed(seg['root'])
        if n:
            return seg['offset'] + n, read(Path(seg['root']) / 'iterations' / f'{n - 1:06d}' / 'complete.json')
    return 0, None


def receipt_at(man, boundary):
    """The completed iteration receipt of boundary b (update b - 1)."""
    seg = next(s for s in reversed(man['segments']) if s['offset'] < boundary)
    return read(Path(seg['root']) / 'iterations' / f'{boundary - 1 - seg["offset"]:06d}' / 'complete.json')


def end_doc(name, arm_name, reason, boundary, receipt, by, **detail):
    return {'schema': END_SCHEMA, 'chain': name, 'arm': arm_name, 'reason': reason, 'last_valid_boundary': boundary,
            'ledger_usec': ((receipt or {}).get('chain_ledger') or {}).get('usec'), 'by': by,
            'unix': round(time.time(), 3), 'detail': detail}


def admitted(cdir):
    path = Path(cdir) / 'ADMITTED.json'
    return read(path)['count'] if path.exists() else 0


def guard_admit(root, cdir, chain):
    """Admission for one segment run (a new segment or a rerun of an unfinished one): a fresh `ok` verdict from
    supervise whose ceiling for this chain is above the chain's admissions so far (ADMITTED.json, counted here).
    Supervise reads every chain's count before it counts bytes and sets the ceiling to it plus GUARD_SEGMENTS, so
    whatever any verdict admits after a census (one run in flight and GUARD_SEGMENTS more, retries and their moved
    aside attempts included) is inside that verdict's reservation, and a stopped supervise admits nothing more."""
    path = Path(root) / 'GUARD.json'
    if not path.exists():
        return False, 'no guard verdict'
    g = read(path)
    if time.time() - g.get('unix', 0) > GUARD_MAX_AGE:
        return False, 'the guard verdict is stale (is supervise running?)'
    if not g.get('ok'):
        return False, f'the byte guard refuses ({g.get("used_bytes")} used)'
    count = admitted(cdir)
    if count >= g.get('ceilings', {}).get(chain, 0):
        return False, f'admission {count} is past this verdict\'s ceiling (is supervise running?)'
    write(Path(cdir) / 'ADMITTED.json', {'count': count + 1})
    return True, 'ok'


def reached_stop(man, ledger_source):
    """The latest boundary's ledger is at or past the ledger source's `stop_at_usec` (resume to R_i). The native
    trainer stops after that boundary; when it was a segment's last update the segment is complete, so the next
    segment must not start (it would run one more update before its own check)."""
    if not ledger_source or not Path(ledger_source).exists():
        return False
    stop = read(ledger_source).get('stop_at_usec')
    _, receipt = latest(man)
    usec = ((receipt or {}).get('chain_ledger') or {}).get('usec')
    return stop is not None and usec is not None and usec >= stop


def end_at_stop(a, arm, root, cdir, man):
    boundary, receipt = latest(man)
    write_once(cdir / 'END.json', end_doc(a.chain, arm['arm'], 'x-stop-at', boundary, receipt, 'driver',
                                          stop_at_usec=read(a.ledger_source)['stop_at_usec']))
    log(root, chain=a.chain, event='end', reason='x-stop-at', boundary=boundary)
    return 0


def current_segment(arm, man, man_path, ledger_source):
    """The segment to run: the manifest's last one while it is unfinished (its config re-derived and compared),
    else a new one appended."""
    cdir = Path(man_path).parent
    length = arm['segment_updates']
    segs = man['segments']
    if segs and not (Path(segs[-1]['root']) / 'completion.json').exists():
        s = len(segs) - 1
    else:
        s = len(segs)
    native = cdir / f'seg-{s:05d}'
    source = next_source(segs[s - 1]['root']) if s > 0 else None
    cfg = arm_config(arm, man['seed_root'], s * length, length, native, source, ledger_source)
    path = cdir / 'segments' / f'seg-{s:05d}.json'
    if path.exists():
        require(read(path) == cfg, f'{path} differs from the segment config derived from the pins')
    else:
        write(path, cfg)
    if s == len(segs):
        require(all(seg['offset'] == i * length for i, seg in enumerate(segs)), 'manifest offsets are not contiguous')
        segs.append({'config': path.as_posix(), 'offset': s * length, 'root': native.as_posix(), 'updates': length})
        write(man_path, man)
    return s, path, native


def drive(a):
    arm = load_arm(a.arm_file)
    root = Path(arm['root'])
    cdir = chain_dir(root, a.chain)
    require(arm['arm'] != 'F', 'filler boxes run through `w5box.py fill`, not as chains')
    man_path = cdir / 'chain.json'
    man = read(man_path)
    require(man['chain'] == a.chain and man['arm_name'] == arm['arm'] and man['seed_root'] == a.seed_root,
            'the chain manifest names another chain, arm or seed root')
    require(man['recipe'] == recipe_identity(arm), 'the arm file changes the recipe of this chain')
    held = lock(cdir / 'DRIVE.lock')
    require(held is not None, f'{a.chain} already has a driver')
    try:
        return drive_locked(a, arm, root, cdir, man_path)
    finally:
        unlock(held)


def drive_locked(a, arm, root, cdir, man_path):
    man = read(man_path)
    if (cdir / 'END.json').exists():
        log(root, chain=a.chain, event='ended-already')
        return 0
    if man['launched_unix'] is None:
        man['launched_unix'] = time.time()
        write(man_path, man)
        log(root, chain=a.chain, event='launched')
    deadline = man['launched_unix'] + arm['c_wall_seconds'] if arm['recipe'] == 'C' else None
    waiting = None
    while True:
        if (cdir / 'END.json').exists():
            return 0
        if stopped(root, cdir):
            log(root, chain=a.chain, event='stop')
            return EXIT_STOP
        if deadline is not None and time.time() >= deadline:
            boundary, receipt = latest(man)
            write_once(cdir / 'END.json', end_doc(a.chain, arm['arm'], 'c-wall', boundary, receipt, 'driver'))
            log(root, chain=a.chain, event='end', reason='c-wall', boundary=boundary)
            return 0
        if reached_stop(man, a.ledger_source):  # a stop at a segment's last update, or a restart past it
            return end_at_stop(a, arm, root, cdir, man)
        ok, why = guard_admit(root, cdir, a.chain)
        if not ok:
            if waiting is None:
                waiting = time.time()
                log(root, chain=a.chain, event='guard-wait', reason=why)
            if time.time() - waiting >= GUARD_WAIT:
                log(root, chain=a.chain, event='no-room', reason=why)
                return EXIT_ROOM
            time.sleep(GUARD_POLL)
            continue
        waiting = None
        s, path, native = current_segment(arm, man, man_path, a.ledger_source)
        moved = quarantine(native, cdir)
        log(root, chain=a.chain, event='segment', segment=s, config=pin(path), moved=moved)
        code, killed = run_trainer(arm, path, deadline, cdir)
        if (native / 'completion.json').exists():
            log(root, chain=a.chain, event='segment-done', segment=s)
            continue
        boundary, receipt = latest(man)
        if killed:
            write_once(cdir / 'END.json', end_doc(a.chain, arm['arm'], 'c-wall', boundary, receipt, 'driver'))
            log(root, chain=a.chain, event='end', reason='c-wall', boundary=boundary)
            return 0
        if code == 0 and reached_stop(man, a.ledger_source):
            return end_at_stop(a, arm, root, cdir, man)
        write(cdir / 'FAILURE.json', {'segment': s, 'exit': code, 'boundary': boundary, 'unix': time.time()})
        log(root, chain=a.chain, event='segment-failed', segment=s, exit=code, boundary=boundary)
        return EXIT_FAILED


def run_filler(arm, out_root, seed_root, stop_check):
    """C-recipe RT1 from the C parent at update 0, unmeasured and discarded: earlier runs in `out_root` are
    removed, and each finished segment's predecessor is deleted (the rolling hot set is the live segment and its
    source)."""
    out_root = Path(out_root)
    out_root.mkdir(parents=True, exist_ok=True)
    for old in out_root.glob('run-*'):
        if old.is_dir() and not is_link(old):
            shutil.rmtree(old)
    run = out_root / f'run-{time.strftime("%Y%m%dT%H%M%S")}-{os.getpid()}'
    length = arm['segment_updates']
    s = 0
    while True:
        if stop_check():
            return EXIT_STOP
        native = run / f'seg-{s:05d}'
        source = next_source(run / f'seg-{s - 1:05d}') if s else None
        path = run / 'segments' / f'seg-{s:05d}.json'
        write(path, arm_config(arm, seed_root, s * length, length, native, source))
        code, _ = run_trainer(arm, path, None, run)
        if not (native / 'completion.json').exists():
            return code or 1
        if s:
            shutil.rmtree(run / f'seg-{s - 1:05d}')
        s += 1


def filler(a):
    arm = load_arm(a.arm_file)
    require(arm['arm'] == 'F', 'the paused-box filler runs arm F')
    return run_filler(arm, Path(arm['root']) / 'fillers' / f'slot{a.slot}', a.seed_root, lambda: False)


# ---------------------------------------------------------------- run, end

def w5box_plan(plan, name):
    return plan['arms'][plan['chains'][name]['arm']]['w5box_plan']['path']


def run(a):
    plan = load_plan(a.plan)
    require(a.chain in plan['chains'], f'the plan has no chain {a.chain}')
    root = Path(plan['root'])
    c = plan['chains'][a.chain]
    require(c['arm'] in MEASURED, f'{a.chain} is a filler box: supervise runs it through `w5box.py fill`')
    require((root / 'plan' / plan['label'] / 'LAUNCHED.json').exists(),
            'launch the stage first (`chains.py launch`): its chains then start held until every box is occupied')
    waiting = [f['slot'] for f in plan['chains'].values()
               if f['arm'] == 'F' and not fill_ready(root, f['slot'], fill_stop(plan))]
    require(not waiting, f'filler boxes {waiting} hold no verified filler yet (supervise starts them)')
    require(read_end(root, a.chain) is None, f'{a.chain} has ended')
    require(not oom_closed(root, a.chain), f'{a.chain} was OOM-killed: a chain failure (section 2.1)')
    require(not stopped(root, chain_dir(root, a.chain)), 'a STOP file is present')
    for p in plan['pairs'].values():
        if p['X'] == a.chain and p['c_external']:
            require(read_end(root, p['C']) is not None, f'the partner {p["C"]} has not ended')
    cmd = [sys.executable, str(HERE / 'w5box.py'), 'run', '--plan', w5box_plan(plan, a.chain), '--chain', a.chain]
    if a.qualification:
        cmd += ['--qualification', a.qualification, '--max-wall-seconds', str(a.max_wall_seconds)]
    if a.accept_lower_bound:
        cmd += ['--accept-lower-bound', a.accept_lower_bound]
    log(root, chain=a.chain, event='run', cmd=cmd)
    code = subprocess.call(cmd)
    log(root, chain=a.chain, event='run-exit', exit=code)
    if read_end(root, a.chain) is not None:
        return 0
    if code == w5box.EXIT_OOM and c['arm'] in MEASURED:
        man = read(chain_dir(root, a.chain) / 'chain.json')
        boundary, receipt = latest(man)
        write_once(chain_dir(root, a.chain) / 'END.json',
                   end_doc(a.chain, c['arm'], 'chain-failure', boundary, receipt, 'run', cause='oom kill'))
    return code


def request(plan, name, what, payload=None):
    """A w5box control request (`pause --now`, `resume --payload`, `kill`) through its own command line."""
    argv = [what, '--plan', w5box_plan(plan, name), '--chain', name]
    if what == 'pause':
        argv.append('--now')
    if payload is not None:
        argv += ['--payload', json.dumps(payload)]
    code = w5box.main(argv)
    log(plan['root'], chain=name, event=f'w5box-{what}', payload=payload, exit=code)
    require(code == 0, f'w5box {what} for {name} exited {code}')
    return code


def ledger_path(root, name):
    return Path(root) / 'state' / name / 'ledger.jsonl'


def live(root, name):
    path = ledger_path(root, name)
    return path.exists() and w5box.Ledger(path).summary()['open'] is not None


def end_cmd(a):
    plan = load_plan(a.plan, check=False)
    require(a.chain in plan['chains'] and plan['chains'][a.chain]['arm'] in MEASURED, 'not a measured chain')
    root = Path(plan['root'])
    man = read(chain_dir(root, a.chain) / 'chain.json')
    top, _ = latest(man)  # receipts only: the index cache is supervise's to write
    boundary = top if a.boundary is None else a.boundary
    require(0 <= boundary <= top, f'boundary must be 0 to {top}')
    receipt = receipt_at(man, boundary) if boundary else None
    doc = end_doc(a.chain, plan['chains'][a.chain]['arm'], 'operator', boundary, receipt, 'operator',
                  note=a.reason)
    if not write_once(chain_dir(root, a.chain) / 'END.json', doc):
        print(json.dumps(read_end(root, a.chain), indent=1))
        return 1
    if live(root, a.chain):
        request(plan, a.chain, 'kill')
    print(json.dumps(doc, indent=1))
    return 0


# ---------------------------------------------------------------- supervise

LIVE_STALE = 30.0  # w5box rewrites an instance's live.json every 5 s while its supervisor runs


def running_intervals(records, now, state=None, evidence=None):
    """Open w5box instances minus the chain's own paused intervals, as (start, end) wall intervals. A chain's driver
    outlives a dead supervisor, so an instance that w5box closed by recovery, or an open one whose live.json is
    stale, ends at its last evidence of execution: the later of its last live reading and its last update receipt
    (`evidence`: instance to unix seconds), else its start."""
    def last_seen(instance, start, default):
        seen = [start]
        path = Path(state) / f'instance-{instance:04d}' / 'live.json' if state is not None else None
        if path is not None and path.exists():
            seen.append(read(path)['unix'])
        if evidence and instance in evidence:
            seen.append(evidence[instance])
        return min(max(seen), default)

    out, opened, start, instance = [], False, None, None
    for r in records:
        event, t = r['event'], r['unix']
        if event == 'open':
            opened, start, instance = True, t, r.get('instance')
        elif event == 'pause' and start is not None:
            out.append((start, t))
            start = None
        elif event == 'resume' and opened and start is None:
            start = t
        elif event == 'close':
            if start is not None:  # a recovery that stopped a running chain: it ran until that close
                end = last_seen(instance, start, t) if r.get('recovered') and not r.get('killed_running') else t
                out.append((start, max(start, end)))
            opened, start = False, None
    if start is not None:
        end = now
        live_path = Path(state) / f'instance-{instance:04d}' / 'live.json' if state is not None and \
            instance is not None else None
        if live_path is not None and (not live_path.exists() or now - read(live_path)['unix'] > LIVE_STALE):
            end = last_seen(instance, start, now)
        out.append((start, max(start, end)))
    return out


def frozen_intervals(root, now):
    """Wall intervals in which w5box's node enforcement held the node's measured chains frozen (amendment A1), from
    its `enforced` events (read without the node lock; a torn last line is ignored)."""
    path = Path(root) / 'state' / 'occupancy.jsonl'
    out, since = [], None
    if not path.exists():
        return out
    for line in path.read_bytes().split(b'\n')[:-1]:
        r = json.loads(line)
        if r.get('event') != 'enforced':
            continue
        if r['frozen'] and since is None:
            since = r['unix']
        elif not r['frozen'] and since is not None:
            out.append((since, r['unix']))
            since = None
    if since is not None:
        out.append((since, now))
    return out


def running_seconds(records, now, frozen=(), state=None, evidence=None):
    """X's own running wall (the cap): its open instances minus its pauses and minus node freezes."""
    total = 0.0
    for a, b in running_intervals(records, now, state, evidence):
        total += b - a
        for fa, fb in frozen:
            total -= max(0.0, min(b, fb) - max(a, fa))
    return total


def valid_records(info):
    end = info['end']
    if end is None or end.get('last_valid_boundary') is None:
        return info['records']
    return [r for r in info['records'] if r['boundary'] <= end['last_valid_boundary']]


def ledger_at(records, boundary):
    return next(r['ledger_usec'] for r in records if r['boundary'] == boundary)


def chain_info(root, name):
    path = chain_dir(root, name) / 'chain.json'
    end = read_end(root, name)  # before the index: an ended chain's index is final
    chain, records = retention.index(path)
    state = Path(root) / 'state' / name
    return {'name': name, 'path': path, 'chain': chain, 'records': records, 'end': end, 'live': live(root, name),
            'paused': (state / 'paused.json').exists(), 'state': state,
            'stop_at': (read(state / 'ledger-source.json').get('stop_at_usec')
                        if (state / 'ledger-source.json').exists() else None),
            'ledger': w5box.Ledger(ledger_path(root, name)).records() if ledger_path(root, name).exists() else []}


def protected_paths(info, boundaries):
    out = set()
    for r in info['records']:
        if r['boundary'] in boundaries:
            out |= {p['path'] for p in retention.record_parts(r)}
    return out


def look(info, r_nom):
    """The 50% R_nom look boundary, recorded once (its checkpoint is protected from every prune)."""
    path = info['path'].parent / 'LOOK.json'
    if path.exists():
        return read(path)
    if r_nom is None:
        return None
    target = LOOK_FRACTION * r_nom
    hit = retention.first_at_or_past(info['records'], target)
    if hit is None:
        return None
    doc = {'boundary': hit['boundary'], 'ledger_usec': hit['ledger_usec'], 'target_usec': target,
           'after_state_sha256': hit['after_state_sha256'], 'checkpoint': hit['checkpoint'], 'state': hit.get('state'),
           'intact': retention.state_intact(hit['checkpoint'], hit.get('state'))}
    write_once(path, doc)
    return doc


def end_x(plan, x, reason, **detail):
    """Ends X (once) and asks w5box to kill a live instance; a kill that does not land is retried every cycle."""
    top = x['records'][-1] if x['records'] else None
    doc = end_doc(x['name'], plan['chains'][x['name']]['arm'], reason, None, None, 'supervise', **detail)
    doc['ledger_usec'] = top and top['ledger_usec']
    if write_once(x['path'].parent / 'END.json', doc) and x['live']:
        request(plan, x['name'], 'kill')


def pair_decide(plan, info_c, info_x, st, now, budget):
    """Before both chains end: the section 2.2 actions for X."""
    root = Path(plan['root'])
    x, c = info_x, info_c
    c_valid = valid_records(c)
    c_ended = c['end'] is not None
    evidence = {}
    for rec in x['records']:
        if rec.get('instance') is not None and rec.get('unix_seconds') is not None:
            evidence[rec['instance']] = max(evidence.get(rec['instance'], 0), rec['unix_seconds'])
    running = running_seconds(x['ledger'], now, frozen_intervals(root, now), x['state'], evidence)
    x_cap = float(x['chain'].get('x_cap_seconds', retention.X_CAP_SECONDS))
    if c_ended:  # R_i is final once C's instance has closed
        r = r_i(root, c)
        if r is None:
            return {'action': 'wait-for-c-close', 'x_running_seconds': running}
        if r == 0:  # C ended early before its first update: X's endpoint is its parent
            end_x(plan, x, 'c-failed-before-update')
            return {'action': 'stop', 'why': 'C ended before its first update'}
        c_valid = [dict(c_valid[-1] if c_valid else {'boundary': 0}, ledger_usec=r)]
    if not c_valid:  # C has no ledger yet: no projection, but the X cap still holds
        if running >= x_cap:
            end_x(plan, x, 'x-cap', x_running_seconds=running)
            return {'action': 'stop_under_budget', 'x_running_seconds': running}
        return {'action': 'wait-for-c', 'x_running_seconds': running}
    made = retention.snapshots(x['path'])
    size = max(s['bytes'] for s in made.values()) if made else st.get('probe_bytes')
    out = retention.decide(x['chain'], x['records'], c['chain'], c_valid, c_ended, running, now, size)
    if size is None and out['action'] == 'run' and 'snapshot_fits' in out:
        # The probe's temporary snapshot is cut from a full state and is smaller than it: reserved at twice the
        # largest full state on disk among the latest boundaries, and returned once the probe file is gone.
        bound = 2 * max([Path(p['path']).stat().st_size for r in x['records'][-65:] for p in retention.record_parts(r)
                         if Path(p['path']).exists()] or [0])
        if bound and budget['headroom'] >= bound:
            spend(budget, bound)
            size = retention.probe_snapshot_bytes(x['records'], plan['container_tool']['path'])
            spend(budget, -bound)
            st['probe_bytes'] = size
            out = retention.decide(x['chain'], x['records'], c['chain'], c_valid, c_ended, running, now, size)
    out['x_running_seconds'] = running
    action = out['action']
    control = x['state'] / 'control'
    if action == 'snapshot' and st.get('snapshot_from') is None:
        rhat = out['projection']['r_hat_usec']
        st['snapshot_from'] = retention.first_at_or_past(x['records'], retention.SNAPSHOT_TRIGGER * rhat)['boundary']
        st['snapshot_window'] = {'r_hat_usec': rhat, 'snapshot_bytes': size,
                                 'projected_updates': out.get('snapshot_window_updates')}
    # Once open, the window stays open while R_i is not final, up to 30 GB of snapshots per chain: each cycle cuts
    # at most as many as the remaining bytes allow at the measured snapshot size.
    if action == 'pause' and x['live'] and not x['paused'] and not (control / 'pause.now').exists():
        request(plan, x['name'], 'pause', {'reason': '1.10 R-hat', 'r_hat_usec': out['projection']['r_hat_usec']})
    elif action == 'resume_to':
        r = int(out['stop_at_usec'])
        if not x['live']:
            out['note'] = 'X has no open instance: start it with `run`'
        elif x['paused']:
            if not (control / 'resume.request').exists():
                request(plan, x['name'], 'resume', {'stop_at_usec': r})
        elif x['stop_at'] != r and not (control / 'pause.now').exists():
            request(plan, x['name'], 'pause', {'reason': 'install stop_at_usec = R_i'})
    elif action in ('select', 'stop_under_budget'):
        end_x(plan, x, 'reached-r' if action == 'select' else 'x-cap', r_usec=out.get('r_usec'),
              x_running_seconds=running)
    # Snapshots are cut by the cycle after every pair's requests (a snapshot backlog never delays a pause or END).
    st['snapshotting'] = st.get('snapshot_from') is not None and not c_ended and not st.get('snapshot_capped')
    st['snapshot_size'] = size
    log(root, chain=x['name'], event='decide', action=action, x_ledger_usec=out.get('x_ledger_usec'),
        r_usec=out.get('r_usec'))
    return out


SNAPSHOT_MARGIN = 1.05  # a weights-only container's size varies only by its metadata


def cut_snapshots(plan, x, st, size, budget):
    """Weights-only snapshots one at a time, each reserved first at the largest size seen plus a margin and
    reconciled with its actual bytes, while the chain's snapshots stay within 30 GB and the cycle's guard budget
    holds one more. A snapshot file left without its index record (a supervise that died mid-cut) is regenerable
    and is removed first."""
    chain = retention.load_chain(x['path'])
    directory = retention.snapshot_dir(x['path'], chain)
    index_file = retention.snapshot_index_path(x['path'])
    def key(path):
        return os.path.normcase(str(Path(path).resolve()))
    indexed = {key(json.loads(line)['path']) for line in index_file.read_text(encoding='utf-8').splitlines() if line} \
        if index_file.exists() else set()
    for orphan in sorted(directory.glob('b*.weights')) if directory.exists() else []:
        if key(orphan) not in indexed:
            orphan.unlink()
            log(plan['root'], chain=x['name'], event='snapshot-orphan-removed', path=orphan.as_posix())
    have = retention.snapshots(x['path'])
    total = sum(v['bytes'] for v in have.values())
    est = int(max([size] + [v['bytes'] for v in have.values()]) * SNAPSHOT_MARGIN) + 1
    made = 0
    while True:
        if total + est > retention.SNAPSHOT_CAP_BYTES:
            st['snapshot_capped'] = True
            break
        if budget['headroom'] < est:
            break
        spend(budget, est)
        new = retention.snapshot(x['path'], plan['container_tool']['path'], st['snapshot_from'], max_new=1)
        if not new:
            spend(budget, -est)
            break
        spend(budget, new[0]['bytes'] - est)
        total += new[0]['bytes']
        est = max(est, int(new[0]['bytes'] * SNAPSHOT_MARGIN) + 1)
        made += 1
    return made


def load_budget(root):
    """The cycle's byte budget: the last ok verdict's headroom less what earlier cycles already spent from that
    verdict (GUARD-SPENT.json, written before each spend, so a supervise restart cannot spend it again)."""
    root = Path(root)
    verdict = read(root / 'GUARD.json') if (root / 'GUARD.json').exists() else {}
    path = root / 'GUARD-SPENT.json'
    spent = read(path).get('spent', 0) if path.exists() and read(path).get('id') == verdict.get('id') else 0
    headroom = verdict.get('headroom', 0) if verdict.get('ok') else 0
    return {'id': verdict.get('id'), 'headroom': headroom - spent, 'spent': spent, 'path': path}


def spend(budget, amount):
    budget['headroom'] -= amount
    budget['spent'] = budget.get('spent', 0) + amount
    if budget.get('path'):
        write(budget['path'], {'id': budget['id'], 'spent': budget['spent']})


def launch_replay(plan, plan_path, rdir, arm, budget):
    """Starts `_replay` detached once the cycle's budget (the last guard verdict's headroom, less the replays this
    cycle has already started) holds its bytes; every later verdict reserves it until its receipt."""
    held = lock(rdir / 'REPLAY.lock')
    if held is None:
        return 'running'
    unlock(held)
    launched = rdir / 'LAUNCHED.json'
    if launched.exists() and time.time() - read(launched)['unix'] < REPLAY_START_GRACE:
        return 'starting'
    need = read(rdir / 'spec.json')['replay']['updates'] * plan['arms'][arm]['bytes_per_update']
    if budget['headroom'] < need:
        return f'waiting for {need} bytes of guard headroom'
    spend(budget, need)
    kwargs = {'creationflags': subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == 'nt' else \
        {'start_new_session': True}
    with open(rdir / 'replay.log', 'ab') as out:
        subprocess.Popen([sys.executable, str(HERE / 'chains.py'), '_replay', '--plan', str(plan_path), '--dir',
                          str(rdir)], stdout=out, stderr=out, stdin=subprocess.DEVNULL, **kwargs)
    write(launched, {'unix': time.time()})
    return 'launched'


def replay_cmd(a):
    plan = load_plan(a.plan)
    rdir = Path(a.dir)
    held = lock(rdir / 'REPLAY.lock')
    require(held is not None, 'a replay is already running here')
    try:
        quarantine(rdir / 'native', rdir)
        arm = {'trainer': plan['trainer']}
        code, _ = run_trainer(arm, rdir / 'config.json', None, rdir)
        write(rdir / 'EXIT.json', {'exit': code, 'unix': time.time()})
        return code
    finally:
        unlock(held)


def select_endpoint(plan, plan_path, info, boundary, budget):
    """SELECTED.json for the chain's endpoint (once): its kept checkpoint, its weights-only snapshot, or a verified
    replay. Returns the record, 'pending' or a blocking reason."""
    cdir = info['path'].parent
    sel_path = cdir / 'SELECTED.json'
    if sel_path.exists():
        sel = read(sel_path)
        return sel if sel['boundary'] == boundary else f'{info["name"]} was selected at {sel["boundary"]}'
    if boundary == 0:
        write_once(sel_path, {'boundary': 0, 'kind': 'parent'})
        return read(sel_path)
    spec = retention.select(info['chain'], info['records'], ledger_at(info['records'], boundary),
                            retention.snapshots(info['path']))
    require(spec['endpoint_boundary'] == boundary, 'the ledger does not identify the endpoint boundary')
    if spec['replay'] is None:
        kind = 'weights' if spec.get('weights_snapshot') else 'checkpoint'
        write_once(sel_path, {'boundary': boundary, 'kind': kind, 'spec': spec})
        return read(sel_path)
    rdir = cdir / 'replay' / f'b{boundary:09d}'
    if not (rdir / 'spec.json').exists():
        write(rdir / 'spec.json', spec)
        write(rdir / 'config.json', retention.replay_config(retention.load_chain(info['path']), spec,
                                                            rdir / 'native'))
    spec = read(rdir / 'spec.json')
    if not (rdir / 'RECEIPT.json').exists():
        if not (rdir / 'EXIT.json').exists():
            return f'pending ({launch_replay(plan, plan_path, rdir, plan["chains"][info["name"]]["arm"], budget)})'
        write(rdir / 'RECEIPT.json', retention.verify_replay(spec, rdir / 'native'))
    receipt = read(rdir / 'RECEIPT.json')
    if not receipt['equal']:
        return f'{info["name"]}: the replay to boundary {boundary} does not reproduce the recorded hashes'
    write_once(sel_path, {'boundary': boundary, 'kind': 'replay', 'spec': spec, 'receipt': receipt})
    return read(sel_path)


def hold_move(root, name, items):
    """Moves each listed file (checked against its pin) into `<root>/hold/<chain>/...` with a move manifest."""
    root = Path(root)
    moved, size = 0, 0
    for item in items:
        p = Path(item['path'])
        if not p.exists():
            continue
        require(not is_link(p) and sha(p) == item['sha256'], f'{p} is a link or differs from its pin')
        require(root.resolve() in p.resolve().parents, f'{p} is outside the W5 root')
        dest = root / 'hold' / name / p.resolve().relative_to(root.resolve())
        dest.parent.mkdir(parents=True, exist_ok=True)
        os.replace(p, dest)
        size += dest.stat().st_size
        with open(root / 'hold' / 'MOVES.jsonl', 'a', encoding='utf-8', newline='\n') as f:
            f.write(json.dumps({'chain': name, 'kind': item.get('kind'), 'boundary': item.get('boundary'),
                                'from': p.as_posix(), 'to': dest.as_posix(), 'sha256': item['sha256'],
                                'bytes': dest.stat().st_size, 'moved_at': time.time()}) + '\n')
        moved += 1
    return moved, size


def purge_hold(root, hold_days, now):
    """Deletes held files `hold_days` after their move, each checked against its pin and inside the hold folder."""
    root = Path(root)
    moves, purged_path = root / 'hold' / 'MOVES.jsonl', root / 'hold' / 'PURGED.jsonl'
    if not moves.exists():
        return 0
    done = {json.loads(x)['to'] for x in purged_path.read_text(encoding='utf-8').splitlines() if x} \
        if purged_path.exists() else set()
    hold = (root / 'hold').resolve()
    count = 0
    for line in moves.read_text(encoding='utf-8').splitlines():
        row = json.loads(line)
        if row['to'] in done or row['moved_at'] > now - hold_days * 86400:
            continue
        p = Path(row['to'])
        if not p.exists():
            continue
        require(not is_link(p) and hold in p.resolve().parents and sha(p) == row['sha256'],
                f'held file {p} is linked, outside the hold folder or changed')
        p.unlink()
        with open(purged_path, 'a', encoding='utf-8', newline='\n') as f:
            f.write(json.dumps({'to': row['to'], 'sha256': row['sha256'], 'bytes': row['bytes'],
                                'purged_at': time.time()}) + '\n')
        count += 1
    return count


def prune_chain(root, info, sel, look_doc):
    """The ARTIFACT-LAW PRUNE manifest (look checkpoints kept) and its moves into the hold folder (once)."""
    cdir = info['path'].parent
    if (cdir / 'PRUNED.json').exists():
        return read(cdir / 'PRUNED.json')
    receipt = sel.get('receipt') if sel['kind'] == 'replay' else None
    man = retention.prune_manifest(info['chain'], info['records'], sel['boundary'], receipt,
                                   retention.snapshots(info['path']))
    if look_doc is not None:
        keep = protected_paths(info, {look_doc['boundary']})
        man['keep']['look_checkpoint'] = [p for p in man['prune'] if p['path'] in keep]
        man['prune'] = [p for p in man['prune'] if p['path'] not in keep]
        man['prune_bytes'] = sum(p['bytes'] for p in man['prune'])
        man['prune_bytes_by_kind'] = {k: sum(p['bytes'] for p in man['prune'] if p['kind'] == k)
                                      for k in ('full', 'weights')}
    write(cdir / 'PRUNE-MANIFEST.json', man)
    moved, size = hold_move(root, info['name'], man['prune'])
    doc = {'manifest': pin(cdir / 'PRUNE-MANIFEST.json'), 'moved': moved, 'bytes': size, 'unix': time.time()}
    write(cdir / 'PRUNED.json', doc)
    return doc


def end_reason(root, info):
    """The chain's END reason, unless w5box recorded an OOM close: then it is a chain failure (section 2.1)."""
    return 'chain-failure' if oom_closed(root, info['name']) else info['end']['reason']


def r_i(root, c):
    """Section 2.2: a C chain that ended at its wall gives R_i = its final ledger value, the closed total of its
    w5box ledger once every instance has closed (the CPU of an update cut at the wall included); a C chain that
    ended early gives the ledger at its last valid checkpoint (0 before its first update). None while C's END is
    written but its instance is still open."""
    c_valid = valid_records(c)
    if end_reason(root, c) != 'c-wall':
        return c_valid[-1]['ledger_usec'] if c_valid else 0
    path = ledger_path(root, c['name'])
    summary = w5box.Ledger(path).summary() if path.exists() else None
    if summary is None or summary['open'] is not None:
        return None
    return max(summary['closed_total_usec'], c_valid[-1]['ledger_usec'] if c_valid else 0)


def pair_finalize(plan, plan_path, c, x, st, looks, budget):
    """Both chains ended and closed: endpoints, selection (replays), PRUNE."""
    c_valid, x_valid = valid_records(c), valid_records(x)
    r = r_i(plan['root'], c)
    require(r is not None, f'{c["name"]} has not closed')
    c_b = c_valid[-1]['boundary'] if c_valid else 0
    c_reason, x_reason = end_reason(plan['root'], c), end_reason(plan['root'], x)
    under = []
    if c_reason != 'c-wall':
        under.append(f'C ended early ({c_reason}): R_i is the ledger at its last valid boundary')
    last = x_valid[-1]['boundary'] if x_valid else 0
    if r == 0:  # C ended early before its first update: R_i = 0, reached at X's boundary 0 (its parent)
        x_b = 0
    elif x_reason in FAILED_ENDS and c_reason == 'c-wall':
        x_b = last  # section 7.4: a failed (or lead-stopped) chain's last valid checkpoint
        under.append(f'X ended by {x_reason}: its endpoint is its last valid boundary')
    else:  # section 2.2, C failing early included: the first valid boundary at or past R_i
        hit = retention.first_at_or_past(x_valid, r)
        x_b = hit['boundary'] if hit else last
        if hit is None:
            under.append(f'X short of R_i ({x_reason})')
    st.update(r_usec=r, endpoints={c['name']: c_b, x['name']: x_b},
              label='under-budget' if under else 'matched', under_budget=under)
    sels = {}
    for info, b in ((c, c_b), (x, x_b)):
        sel = select_endpoint(plan, plan_path, info, b, budget)
        if not isinstance(sel, dict):
            st['waiting'] = sel
            return st
        sels[info['name']] = sel
    st.pop('waiting', None)
    root = Path(plan['root'])
    for info in (c, x):
        st.setdefault('pruned', {})[info['name']] = prune_chain(root, info, sels[info['name']], looks.get(info['name']))
    st['selected'] = {n: {k: v for k, v in s.items() if k != 'spec'} for n, s in sels.items()}
    st['done'] = True
    log(root, event='pair-done', c=c['name'], x=x['name'], label=st['label'], endpoints=st['endpoints'])
    return st


def writing(root, name):
    """A measured chain may still write: it has not ended, or its END is written but its instance is open."""
    return read_end(root, name) is None or live(root, name)


def guard_reservation(plan, root):
    """Bytes the verdict reserves: per measured chain that has not ended, the segment possibly in flight at the
    census plus the GUARD_SEGMENTS its ceiling admits; per box, a filler's whole hot set (two F segments: a filler
    keeps at most its live segment and its source, and does not wait on the guard, which would break occupancy)."""
    def segment(arm):
        return plan['arms'][arm]['segment_updates'] * plan['arms'][arm]['bytes_per_update']
    measured = sum(segment(c['arm']) for n, c in plan['chains'].items() if c['arm'] in MEASURED and writing(root, n))
    boxes = len({c['slot'] for c in plan['chains'].values()})
    replays = 0  # a replay writes until its receipt
    for n, c in plan['chains'].items():
        if c['arm'] in MEASURED:
            for rdir in (chain_dir(root, n) / 'replay').glob('b*'):
                if (rdir / 'spec.json').exists() and not (rdir / 'RECEIPT.json').exists():
                    updates = read(rdir / 'spec.json')['replay']['updates']
                    replays += updates * plan['arms'][c['arm']]['bytes_per_update']
    return (GUARD_SEGMENTS + 1) * measured + 2 * boxes * segment('F') + replays


def supervised(root, name):
    """A w5box supervisor holds the chain's lock (POSIX: a lock taken here would succeed only without one)."""
    path = Path(root) / 'state' / name / 'supervisor.lock'
    if os.name == 'nt' or not path.exists():
        return os.name == 'nt'
    import fcntl
    with open(path, 'a') as f:
        try:
            fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            return True
        fcntl.flock(f, fcntl.LOCK_UN)
        return False


def oom_closed(root, name):
    """w5box closed an instance of the chain after an OOM kill (`chain_failure`), by its supervisor or a recovery."""
    path = ledger_path(root, name)
    return path.exists() and any(r['event'] == 'close' and r.get('chain_failure') for r in w5box.Ledger(path).records())


def end_oom_failures(plan, root):
    """Section 2.1: an OOM kill is a chain failure; its last valid checkpoint is its endpoint (END, once)."""
    ended = []
    for n, c in sorted(plan['chains'].items()):
        if c['arm'] in MEASURED and read_end(root, n) is None and oom_closed(root, n):
            man = read(chain_dir(root, n) / 'chain.json')
            boundary, receipt = latest(man)
            if write_once(chain_dir(root, n) / 'END.json',
                          end_doc(n, c['arm'], 'chain-failure', boundary, receipt, 'supervise', cause='oom kill')):
                log(root, chain=n, event='end', reason='chain-failure', cause='oom kill', boundary=boundary)
                ended.append(n)
    return ended


def recover_unsupervised(plan, root):
    """A measured chain whose ledger is open while no supervisor holds it (its supervisor died) is closed by
    `w5box.py recover`: a still running instance is stopped (nothing reads its requests), its box is held, and the
    instance closes exactly from its cgroup. An unended chain is then restarted by the operator (`run`)."""
    done = []
    for n, c in sorted(plan['chains'].items()):
        if c['arm'] in MEASURED and live(root, n) and not supervised(root, n):
            code = subprocess.call([sys.executable, str(HERE / 'w5box.py'), 'recover', '--plan', w5box_plan(plan, n),
                                    '--chain', n])
            log(root, chain=n, event='recover-unsupervised', exit=code)
            done.append(n)
    return done


def enforce_ends(plan, root):
    """An ended measured chain must not keep running: a kill that did not land (a supervise crash between END and
    the request, or a lost request) is requested again once the END is older than END_KILL_GRACE."""
    kills = []
    for n, c in sorted(plan['chains'].items()):
        end = read_end(root, n) if c['arm'] in MEASURED else None
        control = Path(root) / 'state' / n / 'control'
        if end is not None and live(root, n) and time.time() - end['unix'] > END_KILL_GRACE and \
                not (control / 'kill.request').exists():
            request(plan, n, 'kill')
            kills.append(n)
    return kills


def closed_unix(root, name):
    path = ledger_path(root, name)
    closes = [r['unix'] for r in w5box.Ledger(path).records() if r['event'] == 'close'] if path.exists() else []
    return closes[-1] if closes else None


def fill_stop(plan):
    """The stage's fill stop file: every fill of the plan names it; fills of another stage do not count here."""
    return Path(plan['root']) / 'plan' / plan['label'] / 'fills' / 'STOP'


def fill_ready(root, slot, stop):
    """A filler box holds a verified filler of this stage: its `w5box.py fill` (naming this stage's stop file, which
    is not written) runs and the box has no open outage (the fill opens one before it takes the box and closes it
    only on verification)."""
    if stop.exists() or not fill_running(root, slot, stop):
        return False
    records, torn = w5box.NodeLog(Path(root) / 'state').read()
    return slot not in w5box.NodeLog.state(records, torn)['open_outages']


def fill_running(root, slot, stop=None):
    """`w5box.py fill` holds the box (its mark names a live process, and with `stop`, this stage's stop file)."""
    mark = Path(root) / 'state' / f'box{slot}.fill.json'
    if not mark.exists() or (stop is not None and read(mark).get('stop') != str(stop)):
        return False
    if os.name == 'nt':
        return True
    try:
        os.kill(read(mark)['pid'], 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def launch(a):
    """Opens a launch outage in w5box's node log on every box of the plan that has no live chain, so the stage's
    first chains start frozen (w5box holds a chain while another box's outage is open) and the node thaws once each
    box is occupied: a chain's start restores its own box, a verified filler a filler box."""
    plan = load_plan(a.plan)
    root = Path(plan['root'])
    node = w5box.NodeLog(root / 'state')
    opened = []
    with node.locked():
        records, torn = node.read()
        open_now = w5box.NodeLog.state(records, torn)['open_outages']
        for slot in sorted({c['slot'] for c in plan['chains'].values()}):
            owner = root / 'state' / f'box{slot}.owner.json'
            if slot in open_now or (owner.exists() and live(root, read(owner)['chain'])) or \
                    fill_running(root, slot, fill_stop(plan)):
                continue
            node.append({'event': 'outage-start', 'id': f'launch/{plan["label"]}/{slot}/{time.time_ns()}',
                         'box_slot': slot, 'chain': None, 'launch': True, 'plan': plan['label'],
                         'source': 'w5-chains', 'reason': 'stage launch: the box waits for its chain or filler',
                         'affected': w5box.node_chains(root / 'state')})
            opened.append(slot)
    w5box.enforce_node(read(fill_plan(plan)), node, 'launch')
    write(root / 'plan' / plan['label'] / 'LAUNCHED.json', {'unix': time.time(), 'opened': opened})
    log(root, event='launch', plan=plan['label'], opened=opened)
    print(json.dumps({'launched': plan['label'], 'outages_opened': opened}))
    return 0


def close_handoffs(plan, root):
    """At the stage's end: the handoff outages w5box opened for this plan's boxes as their chains closed (no
    measured chain remains to hold) are closed, so the next stage starts clear."""
    node = w5box.NodeLog(Path(root) / 'state')
    if not node.path.exists():
        return []
    slots = {c['slot'] for c in plan['chains'].values()}
    closed = []
    with node.locked():
        records, torn = node.read()
        for slot, outage in sorted(w5box.NodeLog.state(records, torn)['open_outages'].items()):
            if slot in slots and ((outage.get('handoff') and outage.get('chain') in plan['chains'])
                                  or (outage.get('launch') and outage.get('plan') == plan['label'])):
                node.append({'event': 'restored', 'outage_id': outage['id'], 'box_slot': slot,
                             'chain': outage['chain'], 'how': 'stage-ended', 'source': 'w5-chains'})
                closed.append(slot)
    if closed:
        w5box.enforce_node(read(fill_plan(plan)), node, 'stage-ended')
    return closed


def fill_plan(plan):
    """A measured arm's w5box plan (every plan of a stage carries the same filler and roots)."""
    return next(spec['w5box_plan']['path'] for a, spec in sorted(plan['arms'].items())
                if a in MEASURED and spec['w5box_plan'])


def occupancy(plan, root):
    """Section 2.1 and amendment A1: a filler box (an F chain's slot, Stage 1b) and a box whose measured chain has
    ended (END, instance closed) hold the frozen filler while the stage runs. `w5box.py fill` (started detached
    here, filler boxes first, so measured chains start next to running fillers) runs it under the paused-box
    filler's rules: the box is an outage in the node log, so the node's measured chains stay frozen, until a
    verified filler runs; exits are replaced and repeated failures fault the box (at once escalated in Stage 1b).
    Once every measured chain has ended and closed, the plan's fill STOP file is written."""
    measured = [n for n, c in plan['chains'].items() if c['arm'] in MEASURED]
    closed = {n: read_end(root, n) is not None and not live(root, n) for n in measured}
    out = {'started': [], 'stopped': []}
    stop = fill_stop(plan)  # the plan's fills end with its stage; a fill started late sees it and exits at once
    marks = stop.parent
    if measured and all(closed.values()):
        if not stop.exists():
            marks.mkdir(parents=True, exist_ok=True)
            stop.write_text('')
            out['stopped'] = sorted(c['slot'] for c in plan['chains'].values() if fill_running(root, c['slot'], stop))
        out['handoffs_closed'] = close_handoffs(plan, root)
        return out
    verdict = read(Path(root) / 'GUARD.json') if (Path(root) / 'GUARD.json').exists() else {}
    if not verdict.get('ok') or verdict.get('plan') != plan['label'] or \
            time.time() - verdict.get('unix', 0) > GUARD_MAX_AGE:
        out['held'] = 'no fresh ok guard verdict of this plan: no filler starts (an empty box stays an outage)'
        return out
    boxes = [(c['slot'], n, None) for n, c in sorted(plan['chains'].items()) if c['arm'] == 'F']
    boxes += [(plan['chains'][n]['slot'], n, closed_unix(root, n)) for n in sorted(measured) if closed[n]]
    for slot, n, since in boxes:
        mark = marks / f'box{slot}.json'
        if fill_running(root, slot, stop) or \
                (mark.exists() and time.time() - read(mark)['unix'] < FILL_START_GRACE):
            continue
        cmd = [sys.executable, str(HERE / 'w5box.py'), 'fill', '--plan', fill_plan(plan), '--slot', str(slot),
               '--stop', str(stop)]
        if since is not None:
            cmd += ['--empty-since', repr(since)]
        kwargs = {'creationflags': subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == 'nt' else \
            {'start_new_session': True}
        marks.mkdir(parents=True, exist_ok=True)
        with open(marks / f'box{slot}.log', 'ab') as log_file:
            subprocess.Popen(cmd, stdout=log_file, stderr=log_file, stdin=subprocess.DEVNULL, **kwargs)
        write(mark, {'unix': time.time(), 'for': n})
        log(root, event='fill-start', slot=slot, box_of=n, empty_since=since)
        out['started'].append(slot)
    return out


def cycle(plan, plan_path, now=None, fills=True):
    now = time.time() if now is None else now
    root = Path(plan['root'])
    names = {n for n, c in plan['chains'].items() if c['arm'] in MEASURED}
    names |= {p['C'] for p in plan['pairs'].values()}
    recovered = recover_unsupervised(plan, root)  # before anything reads the chains: dead supervisors' instances
    oom_failures = end_oom_failures(plan, root)  # close, and OOM closes become chain failures, first
    infos = {n: chain_info(root, n) for n in sorted(names)}
    looks = {n: look(i, plan['r_nom_usec']) for n, i in infos.items()}
    report = {'unix': now, 'pairs': {}, 'retention': {}, 'recovered': recovered, 'oom_failures': oom_failures}
    budget = load_budget(root)
    for key, pair in sorted(plan['pairs'].items()):
        c, x = infos[pair['C']], infos[pair['X']]
        pdir = root / 'pairs' / f'{pair["C"]}--{pair["X"]}'
        st = read(pdir / 'PAIR.json') if (pdir / 'PAIR.json').exists() else {
            'schema': PAIR_SCHEMA, 'c': pair['C'], 'x': pair['X'], 'pair': int(key)}
        if not st.get('done'):
            try:
                if x['end'] is None and (x['records'] or x['live']):
                    st['last_decision'] = pair_decide(plan, c, x, st, now, budget)
                    x['end'] = read_end(root, x['name'])
                if c['end'] is not None and x['end'] is not None and not c['live'] and not x['live']:
                    x = infos[pair['X']] = chain_info(root, pair['X'])
                    st = pair_finalize(plan, plan_path, c, x, st, looks, budget)
                st.pop('blocked', None)
            except (Stop, SystemExit) as exc:
                st['blocked'] = str(exc)
            write(pdir / 'PAIR.json', st)
        report['pairs'][key] = {k: st.get(k) for k in ('label', 'r_usec', 'endpoints', 'done', 'waiting', 'blocked')}
        report['pairs'][key]['action'] = (st.get('last_decision') or {}).get('action')
    for key, pair in sorted(plan['pairs'].items()):  # snapshots, once every pair's requests are out
        pdir = root / 'pairs' / f'{pair["C"]}--{pair["X"]}'
        st = read(pdir / 'PAIR.json') if (pdir / 'PAIR.json').exists() else {}
        if st.get('done') or not st.get('snapshotting') or not st.get('snapshot_size'):
            continue
        try:
            report['pairs'][key]['snapshots_made'] = cut_snapshots(plan, infos[pair['X']], st, st['snapshot_size'],
                                                                   budget)
            st.pop('snapshot_error', None)
        except (Stop, SystemExit) as exc:
            st['snapshot_error'] = str(exc)
        write(pdir / 'PAIR.json', st)
    pair_state = {}
    for pair in plan['pairs'].values():
        pdir = root / 'pairs' / f'{pair["C"]}--{pair["X"]}'
        if (pdir / 'PAIR.json').exists():
            pair_state[pair['X']] = read(pdir / 'PAIR.json')
    # The admission census is read before the prune: whatever any verdict admits after it (one run in flight and
    # GUARD_SEGMENTS more) is inside the next verdict's reservation and the per-chain hot cap.
    ceilings = {n: admitted(chain_dir(root, n)) + GUARD_SEGMENTS for n, c in plan['chains'].items()
                if c['arm'] in MEASURED and writing(root, n)}
    for n in sorted(n for n in names if n in plan['chains']):
        info = infos[n]
        if info['end'] is not None or read_end(root, n) is not None or not info['records']:
            continue
        st = pair_state.get(n, {})
        snap_from = st.get('snapshot_from') if st.get('snapshotting') else None
        doc = retention.plan(info['path'], snap_from)
        # The look is found again on an index at least as new as the plan's, so a look boundary the plan already
        # saw is recorded and kept before anything is deleted.
        _, fresh = retention.index(info['path'])
        info = dict(info, records=fresh)
        looks[n] = look(info, plan['r_nom_usec'])
        keep = protected_paths(info, {looks[n]['boundary']} if looks.get(n) else set())
        doc['checkpoints'] = [i for i in doc['checkpoints'] if i['path'] not in keep]
        (root / 'retention').mkdir(parents=True, exist_ok=True)
        removed = retention.apply(doc, root / 'retention' / f'{n}.jsonl')
        report['retention'][n] = {'removed': removed, 'bytes': sum(i['bytes'] for i in doc['checkpoints'])
                                  + sum(i['bytes'] for i in doc['trajectories'])}
    report['kills'] = enforce_ends(plan, root)
    report['fills'] = occupancy(plan, root) if fills else None
    report['purged'] = purge_hold(root, plan['hold_days'], now)
    next_bytes = guard_reservation(plan, root)
    manifests = [c['manifest'] for c in plan['chains'].values() if c['manifest']]
    g = retention.guard([root], plan['byte_cap'], next_bytes, manifests)
    write(root / 'GUARD.json', dict(g, unix=time.time(), plan=plan['label'], ceilings=ceilings, id=time.time_ns(),
                                    headroom=g['cap_bytes'] - g['used_bytes'] - g['next_bytes']))
    report['guard'] = {k: g[k] for k in ('ok', 'used_bytes', 'next_bytes', 'cap_bytes')}
    write(root / 'plan' / plan['label'] / 'SUPERVISE.json', report)
    return report


def supervise(a):
    plan = load_plan(a.plan)
    root = Path(plan['root'])
    held = lock(root / 'SUPERVISE.lock')
    require(held is not None, 'another supervise holds this node')
    try:
        while True:
            report = cycle(plan, Path(a.plan).resolve())
            print(json.dumps(report, sort_keys=True), flush=True)
            if not a.loop:
                return 0
            time.sleep(a.loop)
            plan = load_plan(a.plan)
    finally:
        unlock(held)


# ---------------------------------------------------------------- status, segcheck, fillers

def status(a):
    plan = load_plan(a.plan, check=False)
    root = Path(plan['root'])
    out = {'plan': plan['label'], 'stage': plan['stage'], 'chains': {}, 'pairs': {}}
    for name, c in sorted(plan['chains'].items()):
        row = {'arm': c['arm'], 'slot': c['slot'], 'pair': c['pair'], 'seed_root': c['seed_root'],
               'live': live(root, name), 'paused': (root / 'state' / name / 'paused.json').exists()}
        if c['manifest']:
            man = read(c['manifest'])
            boundary, receipt = latest(man)
            end = read_end(root, name)
            row.update(segments=len(man['segments']), boundary=boundary, launched_unix=man['launched_unix'],
                       ledger_usec=((receipt or {}).get('chain_ledger') or {}).get('usec'),
                       end=None if end is None else end['reason'],
                       failure=(chain_dir(root, name) / 'FAILURE.json').exists(),
                       look=(chain_dir(root, name) / 'LOOK.json').exists(),
                       selected=(chain_dir(root, name) / 'SELECTED.json').exists())
        out['chains'][name] = row
    for key, pair in sorted(plan['pairs'].items()):
        path = root / 'pairs' / f'{pair["C"]}--{pair["X"]}' / 'PAIR.json'
        st = read(path) if path.exists() else {}
        out['pairs'][key] = {k: st.get(k) for k in ('label', 'r_usec', 'endpoints', 'done', 'waiting', 'blocked')}
        out['pairs'][key]['action'] = (st.get('last_decision') or {}).get('action')
    if (root / 'GUARD.json').exists():
        g = read(root / 'GUARD.json')
        out['guard'] = {'ok': g['ok'], 'used_bytes': g['used_bytes'], 'cap_bytes': g['cap_bytes'],
                        'age_seconds': round(time.time() - g['unix'])}
    print(json.dumps(out, indent=1))
    return 0


def segcheck(a):
    """On this host: two updates of the arm's recipe as one native run and as two one-update segments; equal after
    states and trajectories (path and timing fields aside) are the regeneration receipt for segmenting."""
    plan = load_plan(a.plan)
    arm = load_arm(plan['arms'][a.arm]['file']['path'])
    out = Path(plan['root']) / 'segcheck' / f'{a.arm}-{time.strftime("%Y%m%dT%H%M%S")}'
    seed = 'w5/segcheck'

    def go(cfg, label):
        path = out / f'{label}.json'
        write(path, cfg)
        return run_trainer(arm, path, None, out / f'{label}-logs')[0]

    codes = [go(arm_config(arm, seed, 0, 2, out / 'whole'), 'whole'),
             go(arm_config(arm, seed, 0, 1, out / 'seg-a'), 'seg-a')]
    codes.append(go(arm_config(arm, seed, 1, 1, out / 'seg-b', next_source(out / 'seg-a')), 'seg-b'))
    whole = list(qual.iterations(out / 'whole'))
    split = list(qual.iterations(out / 'seg-a')) + list(qual.iterations(out / 'seg-b'))
    states = ([u['after_state_sha256'] for _, u, _ in whole], [u['after_state_sha256'] for _, u, _ in split])
    trajectories = len(whole) == len(split) and all(
        [qual.strip(read(p['path']), qual.PATH_KEYS) for p in x['trajectories']] ==
        [qual.strip(read(p['path']), qual.PATH_KEYS) for p in y['trajectories']]
        for (_, _, x), (_, _, y) in zip(whole, split))
    record = {'schema': SEGCHECK_SCHEMA, 'arm': a.arm, 'arm_file': plan['arms'][a.arm]['file'],
              'trainer_sha256': arm['trainer']['sha256'], 'host_class': plan['host_class'], 'exits': codes,
              'states_whole': states[0], 'states_segmented': states[1],
              'trajectories_equal_apart_from_paths': trajectories,
              'pass': not any(codes) and len(whole) == 2 and states[0] == states[1] and trajectories}
    write(a.out, record)
    print(json.dumps(record, indent=1))
    return 0 if record['pass'] else 1


def clean_fillers(a):
    """A stage's end: every filler tree (`<root>/fillers/slot<k>`) whose filler is not running."""
    plan = load_plan(a.plan, check=False)
    root = Path(plan['root'])
    trees = []
    paused = {c['slot'] for n, c in plan['chains'].items() if (root / 'state' / n / 'paused.json').exists()}
    for slot_dir in sorted((root / 'fillers').glob('slot*')):
        slot = int(slot_dir.name[4:])
        if slot not in paused and not fill_running(root, slot):
            trees.append(slot_dir)
    size = sum(retention.tree_bytes(t) for t in trees)
    if a.apply:
        for t in trees:
            require(not is_link(t), f'{t} is a link')
            shutil.rmtree(t)
        log(root, event='clean-fillers', trees=[t.as_posix() for t in trees], bytes=size)
    print(json.dumps({'trees': [t.as_posix() for t in trees], 'bytes': size, 'deleted': a.apply}, indent=1))
    return 0


# ---------------------------------------------------------------- box throughput (Stage 0)

THROUGHPUT_SEED_ROOT = 'w5/throughput'
THROUGHPUT_QUALIFICATION = 'w5-box-throughput'
THROUGHPUT_POLL = 0.5
THROUGHPUT_CYCLE = 10.0
THROUGHPUT_STOP_GRACE = 600.0
STAGE0_EXIT_SCHEMA = 'w5-stage0-exit/v1'
X_CAP_DAYS = retention.X_CAP_SECONDS / retention.DAY  # 10.5: section 6's launch bound and default cap
RESPONSE_CAP_DAYS = 14.0  # section 6: the cap response applies up to 14 projected days
RESPONSE_CAP_FACTOR = 1.2


def arm_binding(plan, arm):
    """w5box's throughput binding of the arm's launch plan (arm-level: any of its chains gives the same)."""
    spec = (plan['arms'].get(arm) or {}).get('w5box_plan')
    require(spec is not None, f'the plan has no w5box launch plan for arm {arm}')
    box = read(spec['path'])
    require(box['chains'], f'the {arm} launch plan has no chains')
    return w5box.throughput_binding(box, next(iter(box['chains'].values())))


def throughput_inputs(plan, arm, label, count, seed_root):
    """A phase's Stage 0 inputs: the plan's own inputs (so arm file, filler and launch specification are the same
    bytes) with `count` chains of `arm` on slots 0 to count - 1, all on one seed root, and no throughput records
    (a stale one would make w5box check it)."""
    check_pin(plan['inputs'], 'the plan inputs')
    doc = copy.deepcopy(read(plan['inputs']['path']))
    doc['stage'], doc['label'] = '0', f'{label}-{arm.lower()}-p{count}'
    require(NAME.fullmatch(f'{doc["label"]}-s{count - 1}') is not None,
            f'label {label}: phase and chain names must be lowercase letters, digits and dashes (32 at most)')
    for spec in doc['arms'].values():
        spec.pop('throughput', None)
    doc['chains'] = {f'{doc["label"]}-s{k}': {'arm': arm, 'slot': k, 'seed_root': seed_root} for k in range(count)}
    return doc


def throughput_plans(plan_path, arm, label, counts, seed_root=THROUGHPUT_SEED_ROOT):
    """The phase plans, each checked to bind exactly what the plan launches for the arm, with fresh chains."""
    plan = load_plan(plan_path)
    require(arm in MEASURED, f'{arm} is not a measured arm')
    binding = arm_binding(plan, arm)
    require(counts and counts[0] == 1 and counts[-1] == binding['boxes'] and counts == sorted(set(counts)),
            f'phase box counts must rise from 1 to every box ({binding["boxes"]}): {counts}')
    root = Path(plan['root'])
    out = []
    for count in counts:
        doc = throughput_inputs(plan, arm, label, count, seed_root)
        for name in doc['chains']:
            require(not chain_dir(root, name).exists(), f'{name} exists: a throughput label is used once')
        inputs_path = root / 'plan' / doc['label'] / 'INPUTS.json'
        write(inputs_path, doc)
        path, phase = make_plan(inputs_path)
        require(arm_binding(phase, arm) == binding, f'phase {doc["label"]} binds another launch specification')
        out.append((path, phase))
    return plan, binding, out


def node_idle(root):
    """No chain, fill or open outage, fault or freeze on the node: the phases measure the boxes alone."""
    state = Path(root) / 'state'
    owners = [p.name for p in state.glob('box*.owner.json')]
    fills = [p.name for p in state.glob('box*.fill.json')]
    records, torn = w5box.NodeLog(state).read()
    node = w5box.NodeLog.state(records, torn)
    require(not owners and not fills, f'the node runs chains or fills: {owners + fills}')
    require(not node['frozen'], 'the node log has an open outage, fault, freeze or torn tail')


def meminfo():
    try:
        rows = dict(line.split(':', 1) for line in Path('/proc/meminfo').read_text().splitlines() if ':' in line)
    except OSError:
        return {}
    return {k: int(rows[k].split()[0]) * 1024 for k in ('MemAvailable', 'Committed_AS') if k in rows}


def sample_memory(plan, peaks, node):
    """Each running chain cgroup's `memory.peak` (else `memory.current`), and the node's MemAvailable and
    Committed_AS, folded into the phase's extremes."""
    for name, c in plan['chains'].items():
        cg = Path(plan['cgroup_root']) / f'box{c["slot"]}' / 'chain'
        for f in ('memory.peak', 'memory.current'):
            try:
                value = int((cg / f).read_text().split()[0])
            except (OSError, ValueError, IndexError):
                continue
            peaks[name] = max(peaks.get(name) or 0, value)
            break
    info = meminfo()
    if 'MemAvailable' in info:
        node['mem_available_min_bytes'] = min(node.get('mem_available_min_bytes') or info['MemAvailable'],
                                              info['MemAvailable'])
    if 'Committed_AS' in info:
        node['committed_as_max_bytes'] = max(node.get('committed_as_max_bytes') or 0, info['Committed_AS'])


def box_threads(plan, slot):
    return len(read(plan['layout']['path'])['boxes'][slot]['cpus'])


def box_measurement(plan, name, updates, threads, memory_peak=None):
    """One box's completed work over its first `updates` updates (module docstring, "Box throughput")."""
    root = Path(plan['root'])
    c = plan['chains'][name]
    _, records = retention.index(c['manifest'])
    require(len(records) >= updates, f'{name} completed {len(records)} of {updates} updates')
    recs = records[:updates]
    require(all(r['ledger_usec'] is not None and r['unix_seconds'] is not None for r in recs),
            f'{name}: an update receipt lacks its chain ledger')
    require({r['instance'] for r in recs} == {0}, f'{name}: the window spans a launcher restart')
    led = w5box.Ledger(ledger_path(root, name)).records() if ledger_path(root, name).exists() else []
    starts = [r['unix'] for r in led if r['event'] == 'start' and r['instance'] == 0]
    require(starts, f'{name}: no launcher start in its ledger')
    start, end = starts[0], recs[-1]['unix_seconds']
    held = sum(max(0.0, min(end, b) - max(start, a)) for a, b in frozen_intervals(root, end))
    seconds, usec = end - start - held, recs[-1]['ledger_usec']
    require(seconds > 0 and usec > 0, f'{name}: no in-box wall or CPU over the window')
    cpu_per_wall = usec / 1e6 / seconds
    return {'chain': name, 'slot': c['slot'], 'updates': updates, 'seconds': seconds, 'ledger_usec': usec,
            'start_unix': start, 'end_unix': end, 'held_seconds': held,
            'updates_per_wall_second': updates / seconds, 'updates_per_cpu_second': updates / (usec / 1e6),
            'in_box_cpu_per_wall': cpu_per_wall, 'in_box_utilization': cpu_per_wall / threads,
            'memory_peak_bytes': memory_peak,
            'identity': {f'b{r["boundary"]:09d}': r['after_state_sha256'] for r in recs}}


def throughput_record(binding, phases, meta):
    """The `w5-box-throughput/v1` record, checked by w5box's own guard rule before it is returned."""
    record = {'schema': w5box.THROUGHPUT_SCHEMA, 'binding': binding, 'phases': phases, 'qualification': meta}
    try:
        guard = w5box.require_throughput(record, binding)
    except (w5box.Refusal, KeyError, IndexError) as exc:
        raise Stop(f'the record fails the launcher guard: {exc}')
    serial = phases[0]['boxes'][0]['updates_per_wall_second']
    full = phases[-1]['boxes']
    record['summary'] = dict(guard, serial_updates_per_second=serial,
                             full_node_speedup=guard['full_node_updates_per_second'] / serial,
                             per_box_rate_vs_serial=guard['full_node_updates_per_second'] / len(full) / serial,
                             in_box_utilization=[b['in_box_utilization'] for b in full])
    record['pass'] = True
    return record


def identity_differences(phases):
    ref = phases[0]['boxes'][0]['identity'] if phases and phases[0]['boxes'] else {}
    out = []
    for phase in phases:
        for b in phase['boxes']:
            diff = sorted(k for k in set(ref) | set(b['identity']) if ref.get(k) != b['identity'].get(k))
            if diff:
                out.append({'chain': b['chain'], 'first': diff[0], 'count': len(diff)})
    return out


def spawn_run(plan_path, name, max_wall):
    """The chain through `chains.py run` (and so the guarded w5box launcher) in qualification mode."""
    return subprocess.Popen([sys.executable, str(HERE / 'chains.py'), 'run', '--plan', str(plan_path), '--chain',
                             name, '--qualification', THROUGHPUT_QUALIFICATION, '--max-wall-seconds',
                             str(max_wall)], stdin=subprocess.DEVNULL)


def launch_phase(plan_path):
    old, sys.stdout = sys.stdout, open(os.devnull, 'w')
    try:
        return launch(argparse.Namespace(plan=str(plan_path)))
    finally:
        sys.stdout.close()
        sys.stdout = old


def stop_chains(plan, names):
    for n in names:
        p = chain_dir(plan['root'], n) / 'STOP'
        if not p.exists():
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('throughput window complete\n')


def reached(plan, name, updates):
    """The chain has completed update N - 1 (boundary N): its receipt exists (N is whole segments)."""
    length = plan['arms'][plan['chains'][name]['arm']]['segment_updates']
    return (chain_dir(plan['root'], name) / f'seg-{updates // length - 1:05d}' / 'iterations' /
            f'{length - 1:06d}' / 'complete.json').exists()


def end_phase(plan, procs, poll):
    """Every chain of the phase goes at once: STOP files (no new segment, no rerun), then w5box kills. A chain
    stopped alone would leave its box a handoff outage that freezes the rest of the node. Returns the chains still
    running after the grace."""
    stop_chains(plan, procs)
    for n, p in procs.items():
        if p.poll() is None and live(plan['root'], n):
            try:
                request(plan, n, 'kill')
            except Stop as exc:
                log(plan['root'], chain=n, event='throughput-kill-failed', reason=str(exc))
    deadline = time.monotonic() + THROUGHPUT_STOP_GRACE
    for p in procs.values():
        while p.poll() is None and time.monotonic() < deadline:
            time.sleep(poll)
    return sorted(n for n, p in procs.items() if p.poll() is None)


def run_phase(plan_path, plan, updates, max_wall, spawn=spawn_run, launcher=launch_phase, poll=THROUGHPUT_POLL):
    """One phase: a guard verdict, the launch hold and every chain started; the node runs whole until every box has
    completed N updates (so each box's window has every other box busy), then all chains end together. Supervise
    cycles run throughout without fills (a qualification box holds no filler). A chain that exits before every box
    is done fails the phase."""
    names = sorted(plan['chains'])
    cycle(plan, plan_path, fills=False)  # a guard verdict and admission ceilings before any driver starts
    launcher(plan_path)
    procs, peaks, node, short, last = {}, {}, {}, None, time.monotonic()
    try:
        for n in names:
            procs[n] = spawn(plan_path, n, max_wall)
        while True:
            done = {n for n in names if reached(plan, n, updates)}
            if len(done) == len(names):
                break
            # Any exit before every box is done ends the phase: the exited box's handoff outage freezes its peers
            # (one that exits after its own N included).
            gone = sorted(n for n in names if procs[n].poll() is not None)
            if gone:
                short = {n: procs[n].returncode for n in gone}
                break
            sample_memory(plan, peaks, node)
            if time.monotonic() - last >= THROUGHPUT_CYCLE:
                cycle(plan, plan_path, fills=False)
                last = time.monotonic()
            time.sleep(poll)
    finally:
        running = end_phase(plan, procs, poll)
    exits = {n: p.returncode for n, p in procs.items()}
    if short:
        raise Stop(f'{sorted(short)} exited {short} before every box completed {updates} updates')
    require(not running, f'{running} did not end within {THROUGHPUT_STOP_GRACE:g} s of the kill')
    boxes = [box_measurement(plan, n, updates, box_threads(plan, plan['chains'][n]['slot']), peaks.get(n))
             for n in names]
    return {'concurrent_boxes': len(names), 'label': plan['label'], 'plan': pin(plan_path), 'exits': exits,
            'node_memory': node, 'boxes': sorted(boxes, key=lambda b: b['slot'])}


def drop_trees(plan):
    """A passing record's qualification chains: their segment trees (regenerable bulk) are removed."""
    removed = 0
    for n in plan['chains']:
        for seg in sorted(chain_dir(plan['root'], n).glob('seg-*')):
            if seg.is_dir() and not is_link(seg):
                removed += retention.tree_bytes(seg)
                shutil.rmtree(seg)
    return removed


def throughput_run(plan_path, arm, label, updates, out, counts=None, max_wall=w5box.UNQUALIFIED_MAX_WALL,
                   seed_root=THROUGHPUT_SEED_ROOT, spawn=spawn_run, launcher=launch_phase, poll=THROUGHPUT_POLL,
                   keep_trees=False):
    """`chains.py throughput` (module docstring): phases, measurements, the record. Returns (passed, record)."""
    target = load_plan(plan_path)
    require(arm in target['arms'], f'the plan has no arm {arm}')
    boxes = len(read(target['layout']['path'])['boxes'])
    counts = counts or [1, boxes]
    length = target['arms'][arm]['segment_updates']
    require(updates % length == 0 and updates >= 2 * length,
            f'N must be whole {arm} segments of {length} updates, at least two')
    require(0 < max_wall <= w5box.UNQUALIFIED_MAX_WALL, 'a phase wall cap is at most the unqualified 6 hours')
    root = Path(target['root'])
    require(not (root / 'STOP').exists(), 'a node STOP file is present')
    held = lock(root / 'SUPERVISE.lock')
    require(held is not None, 'another supervise holds this node')
    phases, plans, started, failure = [], [], time.time(), None
    try:
        node_idle(root)
        target, binding, derived = throughput_plans(plan_path, arm, label, counts, seed_root)
        for path, phase in derived:
            plans.append(phase)
            phases.append(run_phase(path, phase, updates, max_wall, spawn, launcher, poll))
    except Stop as exc:
        if not plans:  # refused before any chain ran
            raise
        failure = f'phase {plans[-1]["label"]}: {exc}'
    finally:
        for phase in plans:
            close_handoffs(phase, root)
        unlock(held)
    uname = os.uname() if hasattr(os, 'uname') else None
    meta = {'plan': pin(plan_path), 'label': label, 'arm': arm, 'updates': updates, 'seed_root': seed_root,
            'counts': counts, 'segment_updates': length, 'box_threads': box_threads(target, 0),
            'host_class': target['host_class'], 'hostname': uname and uname.nodename,
            'kernel': uname and uname.release, 'tools': tools_shas(), 'started_unix': started,
            'finished_unix': time.time()}
    try:
        require(failure is None, failure)
        record = throughput_record(binding, phases, meta)
        passed = True
    except Stop as exc:
        record = {'schema': w5box.THROUGHPUT_SCHEMA, 'binding': binding, 'phases': phases, 'qualification': meta,
                  'pass': False, 'why': str(exc), 'identity_differences': identity_differences(phases)}
        passed = False
    if passed:
        record['qualification']['removed_tree_bytes'] = 0 if keep_trees else sum(drop_trees(p) for p in plans)
        write(out, record)
    else:
        write(Path(f'{out}.failed.json'), record)
    log(root, event='throughput', arm=arm, label=label, passed=passed, out=str(out))
    return passed, record


def throughput(a):
    require(sys.platform.startswith('linux'), 'the box throughput qualification runs the W5 launcher (Linux only)')
    counts = [int(x) for x in a.counts.split(',')] if a.counts else None
    passed, record = throughput_run(a.plan, a.arm, a.label, a.updates, a.out, counts, a.max_wall_seconds,
                                    a.seed_root, keep_trees=a.keep_trees)
    print(json.dumps({'pass': passed, 'summary': record.get('summary'), 'why': record.get('why'),
                      'out': a.out if passed else f'{a.out}.failed.json'}, indent=1))
    return 0 if passed else 1


def stage0_exit(records, r_nom_usec=None):
    """Section 6's Stage 0 exit numbers from box throughput records (reported, never decided here)."""
    arms, shapes = {}, set()
    for rec in records:
        try:
            w5box.require_throughput(rec, rec['binding'])
            threads = rec['qualification']['box_threads']
        except (w5box.Refusal, KeyError, TypeError, IndexError) as exc:
            raise Stop(f'not a passing w5-box-throughput/v1 record: {exc}')
        b = rec['binding']
        require(b['arm'] in MEASURED and b['arm'] not in arms, f'a second record of arm {b["arm"]}, or not an arm')
        shapes.add((b['host_class'], b['positions_sha256'], b['cores_per_box'], b['boxes']))
        full, serial = rec['phases'][-1]['boxes'], rec['phases'][0]['boxes'][0]
        rates = [x['ledger_usec'] / x['seconds'] for x in full]
        arms[b['arm']] = {
            'box_cpu_usec_per_wall_second_mean': sum(rates) / len(rates),
            'box_cpu_usec_per_wall_second_min': min(rates),
            'in_box_utilization_mean': sum(rates) / len(rates) / 1e6 / threads,
            'in_box_utilization_min': min(rates) / 1e6 / threads,
            'in_box_utilization_serial': serial['ledger_usec'] / serial['seconds'] / 1e6 / threads,
            'updates_per_wall_second_per_box': sum(x['updates'] / x['seconds'] for x in full) / len(full),
            'updates_per_cpu_second': sum(x['updates'] / (x['ledger_usec'] / 1e6) for x in full) / len(full),
            'box_threads': threads, 'boxes': b['boxes'], 'cores_per_box': b['cores_per_box'],
            'memory_peak_bytes_max': max([x.get('memory_peak_bytes') or 0 for x in full]) or None,
            'node_mem_available_min_bytes': (rec['phases'][-1].get('node_memory') or {}).get('mem_available_min_bytes'),
            'binding': b}
    require(len(shapes) == 1, 'the records measure different hosts or box layouts')
    host, _, cores, boxes = shapes.pop()
    if r_nom_usec is None:
        require('C' in arms, 'R_nom needs --r-nom-usec or a C record')
        r_nom_usec = arms['C']['box_cpu_usec_per_wall_second_mean'] * C_WALL
        r_source = 'provisional: the C record\'s mean full-node box CPU rate x the 7-day C wall'
    else:
        require(r_nom_usec > 0, 'R_nom must be positive')
        r_source = 'given'
    days = {}
    for name, arm in sorted(arms.items()):
        if name == 'C':
            continue
        arm['projected_days_to_r_nom'] = r_nom_usec / arm['box_cpu_usec_per_wall_second_mean'] / retention.DAY
        arm['projected_days_slowest_box'] = r_nom_usec / arm['box_cpu_usec_per_wall_second_min'] / retention.DAY
        days[name] = arm['projected_days_to_r_nom']
    reading = None
    if days:
        worst = max(days.values())
        if worst <= X_CAP_DAYS:
            reading = {'branch': 'launch', 'text': f'every measured X arm projects at most {X_CAP_DAYS:g} days'}
        elif worst <= RESPONSE_CAP_DAYS:
            cap = math.ceil(RESPONSE_CAP_FACTOR * worst)
            reading = {'branch': 'cap', 'x_cap_days': cap,
                       'text': f'the X cap becomes ceil({RESPONSE_CAP_FACTOR:g} x {worst:.3f}) = {cap} days'}
        elif cores == 2:
            reading = {'branch': 'widen', 'text': 'every box widens to 3 cores (4 boxes per node); Stage 0 '
                                                  'remeasures'}
        else:
            reading = {'branch': 'stop', 'text': f'the worst X arm projects {worst:.3f} days at {cores}-core boxes: '
                                                 'W5 stops for a design amendment'}
        reading.update(worst_arm=max(days, key=days.get), worst_days=worst)
    return {'schema': STAGE0_EXIT_SCHEMA, 'host_class': host, 'cores_per_box': cores, 'boxes': boxes,
            'r_nom_usec': r_nom_usec, 'r_nom_source': r_source, 'arms': arms, 'declared_response_reading': reading,
            'x_arms_not_measured': [a for a in ('X8', 'XS') if a not in arms],
            'note': 'Reported for the lead, who applies the declared response at freeze. A single-arm full node '
                    'stands in for Stage 1\'s mixed node.'}


def stage0_exit_cmd(a):
    report = stage0_exit([read(p) for p in a.record], a.r_nom_usec)
    report['records'] = [pin(p) for p in a.record]
    write(a.out, report)
    print(json.dumps({k: report[k] for k in ('r_nom_usec', 'r_nom_source', 'declared_response_reading',
                                             'x_arms_not_measured')}, indent=1))
    return 0


# ---------------------------------------------------------------- main

def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    sub.add_parser('plan').add_argument('--inputs', required=True)
    p = sub.add_parser('run')
    p.add_argument('--plan', required=True)
    p.add_argument('--chain', required=True)
    p.add_argument('--qualification')
    p.add_argument('--max-wall-seconds', type=float)
    p.add_argument('--accept-lower-bound')
    sub.add_parser('launch').add_argument('--plan', required=True)
    p = sub.add_parser('supervise')
    p.add_argument('--plan', required=True)
    p.add_argument('--loop', type=float, nargs='?', const=SUPERVISE_EVERY, default=None)
    sub.add_parser('status').add_argument('--plan', required=True)
    p = sub.add_parser('end')
    p.add_argument('--plan', required=True)
    p.add_argument('--chain', required=True)
    p.add_argument('--reason', required=True)
    p.add_argument('--boundary', type=int)
    p = sub.add_parser('segcheck')
    p.add_argument('--plan', required=True)
    p.add_argument('--arm', required=True, choices=sorted(ARMS))
    p.add_argument('--out', required=True)
    p = sub.add_parser('clean-fillers')
    p.add_argument('--plan', required=True)
    p.add_argument('--apply', action='store_true')
    p = sub.add_parser('throughput')
    p.add_argument('--plan', required=True)
    p.add_argument('--arm', required=True, choices=MEASURED)
    p.add_argument('--label', required=True)
    p.add_argument('--updates', type=int, required=True)
    p.add_argument('--out', required=True)
    p.add_argument('--counts')
    p.add_argument('--max-wall-seconds', type=float, default=w5box.UNQUALIFIED_MAX_WALL)
    p.add_argument('--seed-root', default=THROUGHPUT_SEED_ROOT)
    p.add_argument('--keep-trees', action='store_true')
    p = sub.add_parser('stage0-exit')
    p.add_argument('--record', action='append', required=True)
    p.add_argument('--r-nom-usec', type=int)
    p.add_argument('--out', required=True)
    p = sub.add_parser('_drive')
    for flag in ('--arm-file', '--chain', '--seed-root', '--state', '--ledger-source'):
        p.add_argument(flag, required=True)
    p = sub.add_parser('_filler')
    for flag in ('--arm-file', '--seed-root', '--slot', '--seq'):
        p.add_argument(flag, required=True)
    p = sub.add_parser('_replay')
    p.add_argument('--plan', required=True)
    p.add_argument('--dir', required=True)
    a = ap.parse_args(argv)
    if a.cmd == 'run' and (a.qualification is None) != (a.max_wall_seconds is None):
        ap.error('--qualification and --max-wall-seconds go together')
    try:
        if a.cmd == 'plan':
            path, plan = make_plan(a.inputs)
            print(json.dumps({'plan': path.as_posix(), 'chains': sorted(plan['chains']), 'pairs': plan['pairs'],
                              'xs_recipe_differences': plan['xs_recipe_differences']}, indent=1))
            return 0
        return {'run': run, 'launch': launch, 'supervise': supervise, 'status': status, 'end': end_cmd,
                'segcheck': segcheck, 'clean-fillers': clean_fillers, 'throughput': throughput,
                'stage0-exit': stage0_exit_cmd, '_drive': drive, '_filler': filler,
                '_replay': replay_cmd}[a.cmd](a)
    except Stop as exc:
        print(f'refused: {exc}', file=sys.stderr)
        return w5box.EXIT_REFUSED


if __name__ == '__main__':
    raise SystemExit(main())
