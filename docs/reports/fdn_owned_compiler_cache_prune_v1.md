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