# Owned FDN compiler cache prune manifest

Scope: only these uncited incremental compiler directories, after both owned
qualification guards have ended. No source, executables, PDBs, generated card
definitions, logs, measurements or failure records are deleted.

- C:/mtg-node/codex-fdn-fight-target-20261010/debug/incremental
- C:/mtg-node/codex-fdn-v63-target-20261010/debug/incremental

Native19 guardeaa94dbba304467eb8366c9621b4b9d5 and Native14
guard59fea122e3a347d4b89716994dd3fa3e both ended0. Source heads are2bf6a03e
and2227766a. Both targets belong to this issue110 lane. ARTIFACT-LAW clause4
permits pruning uncited bulk; this is a manual owned-job cache closure, not a
change to the seven-day automatic retention policy.

Before pruning, the script records exact absolute paths, per-file bytes and
SHA256s in C:/mtg-node/fdn110-owned-incremental-prune1-manifest.json and hashes
all retained executables and generated definitions. It rejects reparse points,
keep markers, protected binaries and any target outside the two named roots.
After pruning it verifies every retained identity and records observed free
space. The manifest and receipt will be copied to E:/storage-records/fdn110/.

Regeneration: retained source plus Cargo.lock, Rust1.94.1(e408947bf),
MSVC19.50.35725/14.50.35717, and the recorded native commands regenerate the
compiler cache. Subsequent checks set CARGO_INCREMENTAL=0 to avoid accumulating
this intermediate data. No execution time or regenerated output equivalence
is inferred. Actual prune execution and reserve restoration remain pending.
Executed prune1 under supported guardccb9198a40bc4dd494aab5e6e4329e93,
supervisor152872, cores14-15 BelowNormal, terminal0. Removed23,621 uncited
incremental files totaling37,622,644,719 logical bytes. All114 retained identities
matched; free space rose54.437 to66.713GiB. The per-file manifest SHA256 is
b9a9f05709fd74863db98df8d913eddc79457278e61391898da1d49509c3875a. Manifest
and receipt cold copies in E:/storage-records/fdn110 match that hash. Native15
then admitted cores14-15 under guard23cdb1287c41437cbc0c08ad11cf876d,
supervisor158680, source0295d5f9, with CARGO_INCREMENTAL=0. Its result is pending.

Native15 ended with exit 101 after the combined Koma fixture tried a card absent
from that catalog. Native16 qualified the corrected feature guard and affected
tests; its remaining fixture lint errors are recorded in the ferocious design.
Selective compression6 touched only fresh files in the two idle owned caches.
All 121 retained identities matched, and free space rose from 58.386 to
70.858 GiB. Guard 0e1e2c5114264829a1ce2822b6aefa9b ended with exit 0. The
identity manifests and command logs remain at C:/mtg-node/fdn110-owned-cache-compression6*.
