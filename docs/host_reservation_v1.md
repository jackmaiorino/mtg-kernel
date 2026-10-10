# Windows host reservation release

`python/tools/host_reservation_v1.py` contains the reservation contract and CLI.
Job containment and exact recorded process identities provide the proof that
work has ended. The process tree is an additional conservative refusal check.

Supervisors record each direct child's actual exit FILETIME through its retained
process handle. Nested supervisors record a completion time after their job
empties, immediately before returning without creating further work. Once a
recorded root is absent, the Windows tree walk rejects children born after that
root's exit or completion bound. This handles chained pid reuse, including a
replacement process that has also exited. Children with unreadable identities
still refuse release. Reservations recorded by older supervisors retain the
conservative behavior when no exit bound exists.

An owner can audit an individual false positive using the reservation token:

```text
release --token TOKEN --outcome cancelled --ignore-descendant PID:CREATION
```

Repeat `--ignore-descendant` for each process to ignore. Copy each exact pid and
creation time from `status` and inspect its ownership before choosing the
override. WMI helpers, conhost and another lane's queued claim can produce false
positives in the extra tree walk. They are not automatically excluded by image
name or another registry's claim, which could be stale or incomplete.

The override changes only the tree refusal for each explicitly named identity.
It never ignores that process's descendants implicitly, live or unknown recorded
work, unreadable events, or members of the caller supervisor's Windows job. An
absent, reused, unknown, or unauditable identity is refused. The release event
records every ignored process's exact identity and executable image, read
through one process handle to prevent pid reuse between the identity and image
queries. This command does not terminate processes or rewrite the lock. Reclaim
has no descendant override.
Linux refuses the override because its session/subreaper containment is weaker
than a Windows job object.
