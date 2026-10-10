# Felidar Savior source preparation

This issue110 preparation tentatively follows Pup365 as Felidar Savior366.
Registration and card gameplay qualification remain pending after v64/v65.

[Pinned XMage FelidarSavior.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/f/FelidarSavior.java)
defines a 3W 2/3 Cat Beast with Lifelink. Its ETB puts one +1/+1 counter on
each of up to two other target controlled creatures. The source was read at
that exact commit. Cat and Beast already exist; target59 is appended.

Targeting reuses Pup's captured source incarnation, optional completion and
source-aware pending-prefix validation. Each existing counter leaf is guarded
by a new appended condition that checks current control, creature type,
protection and exact target incarnation against the captured ability source.
Thus one illegal target is skipped while another still resolves. Legacy
counter effects retain their behavior. No serialized context/state field or
accepted catalog entry changes. Rules-vector reads record the control/zone
predicate and explicitly mark the finer legality checks Opaque.

A registered-card effect regression covers both seats, source reentry,
current target control, protection, exile and leave/return, preserving the
legal recipient and restored output. It also rejects missing source
provenance, absent target slots and the original source incarnation. Five
unregistered card fixtures cover exact metadata/payment, zero/one/two choices,
duplicate/self/land/opponent rejection, partial-prefix restore, individually
stale targets, source departure/reentry, forged restored self-target and
actual unblocked Lifelink combat. They need registration before execution.
Native source checks and read-only review are pending. No experiment or paid
execution is included.

Read-only review of71453915 found no actionable defects. That source passed
both test-compilation configurations, the registered Pup targeting regression,
the individual-guard effect regression,23 rules-vector cases (one existing
ignore) and the frozen v63 catalog case under supported guard
13c9f74f04e04d84b87bfdbc2d34e87f, command exit0. The five Felidar card cases
compiled but remain unexecuted until registration. The guard's owned idle
compiler telemetry child was released after verifying its terminal sequence.
