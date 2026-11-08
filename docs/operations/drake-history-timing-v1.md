# Bounded Drake history comparison

`python/tools/drake_history_timing_v1.py` compares the retained Windows
`384c8a4e5103f40384928adb69720268e4ecd8cb` binary with
`17856aa9b6e6c116440b64acab0dcbfe4306e03a`. It does not build either binary,
modify either source tree, acquire a reservation or allocate cloud resources.
No performance or four-game acceptance result exists until execution completes.

The fixed cases are train pairs 241, 436, 597 and eval pair 458, seed 1,
leg 0, random versus random, rules-only-all, 100 turns / 6000 decisions.
Their expected winners and decision counts are P1/1344, P1/1575, P1/1657
and P0/1322. Each exact ignored test runs borrowed and owned replay staging,
clears its replay caches, asserts complete final-state byte equality and
prints two timing receipts. Both binaries must run: borrowed staging in
the new binary still uses the new history storage.

Prepare a fresh availability JSON with `desktop`, `computehost`, `runpod`
entries. Each contains `eligible` (boolean), `reason`, `checked_epoch`,
and `evidence` (`path`, `sha256`) pointing to the current actual resource
inspection. Include host/core topology, competing work, memory, disk,
connectivity and relevant spending authority in those evidence files.
The runner checks evidence pins and freshness. It cannot discover or
qualify remote placement itself. Final comparison refuses another eligible
placement until the coordinator has resolved that missing qualification;
do not mark available capacity ineligible merely to pass the check.

Choose two free logical CPUs of the same performance class on different
physical cores, using the current host topology and reservation. The
runner fixes the first case to the first CPU and the second case to the
second CPU, including during the serial run. Old and new therefore use
the same core for each case. The coordinator must preserve this placement
and competing-load envelope when reusing qualification evidence. Reserve
both worker CPUs and their SMT siblings so another claim cannot share a
physical core. For desktop worker CPUs `0,2`, the current topology requires
reserved CPUs `0,1,2,3`. Confirm that topology at placement time. The plan
pins worker placement separately from the complete reservation; every
batch receipt binds that exact plan.

Offline preparation, with an existing output parent:

```text
python -B python/tools/drake_history_timing_v1.py prepare --output D:/OWNED/plan.json --cores 0,2 --reserved-cores 0,1,2,3 --availability D:/OWNED/availability.json
```

Preparation pins both known binary hashes, Python, runner/guard sources,
the complete runtime pool and deck files. These files are read from each
binary's compiled CARGO_MANIFEST_DIR, not from the runner's cwd. Moving a
binary does not redirect its inputs. A changed binary, source helper or
runtime input invalidates the plan. Preserve those original worktrees.

Execute only under the supported `host_reservation_v1` supervisor, with
`host_slots_v1.py timed --cores 0,1,2,3 --` wrapping the command below,
matching the plan's complete reservation.
Use the coordinator's existing supervisor launch procedure. The runner
requires the live reservation token, supervised ancestry and exact live
timed declaration. An ordinary slot claim is insufficient for timing.
The existing nested Windows job helper confines every native worker to
its assigned core at BelowNormal priority and cleans up descendants.

```text
python -B python/tools/drake_history_timing_v1.py execute --plan D:/OWNED/plan.json --sha256 PLAN_SHA --output D:/OWNED/qualification --stage qualify
python -B python/tools/drake_history_timing_v1.py execute --plan D:/OWNED/plan.json --sha256 PLAN_SHA --output D:/OWNED/comparison --stage compare --qualification D:/OWNED/qualification/qualification.json
```

Qualification runs the first two cases on both binaries with one worker,
then repeats exactly those jobs with two workers. It checks actual overlap,
identical final hashes and complete work, including process startup,
input validation, native execution, log storage and cleanup. The lower
completed-batch wall time selects one or two workers. The final comparison
reuses that selected batch, checks all its pinned logs again, and runs only
train 597 and eval 458. This is 16 qualification games plus 8 remaining
games, not a new strength campaign. No synthetic load or learning occurs.

Each invocation is limited to 1800 seconds, each batch to 7200 seconds,
each stage to 1 GiB output and at least 60 GiB free space. Guard loss, STOP,
deadline, changed input or failed test aborts the batch and invokes bounded
job cleanup. Preserve incomplete logs and receipts. Failed or zero-test
runs never become successful comparison data. `MTG_LIMITED_MATCH_TRACE`
is removed from native environments.

`windows.csv/json` contains every timing window, history length and Drake
row count, suitable for plotting seconds per 100 decisions versus decision
position or history length. `paired_windows.csv/json` pairs old/new windows;
`comparison.json` binds the source batches and confirms final hash equality.
Partial final windows are marked and normalized by their actual decision
count. They must not be interpreted as full 100-decision samples. Inspect
all windows and the issue's 50/70/90 percent regions. A lower total runtime
alone does not satisfy the requirement that late-game cost stop rising.
The tool deliberately does not invent a slope threshold or claim flat cost.

Offline re-analysis of completed receipts:

```text
python -B python/tools/drake_history_timing_v1.py analyze --plan D:/OWNED/plan.json --sha256 PLAN_SHA --qualification D:/OWNED/qualification/qualification.json --remaining D:/OWNED/comparison/remaining/batch.json --output D:/OWNED/reanalysis
```

The pure parser tests do not launch games or acquire host resources.
Native execution and serial/parallel qualification remain pending.
