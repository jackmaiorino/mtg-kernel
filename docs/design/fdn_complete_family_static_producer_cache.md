# Static producer hashing in complete family inputs

Audit base: `07f1494b435783e4eb6540f01255b736bedf5375`. This is a
source-level call-path and immutable-byte audit, with Rust uncompiled/unrun.
No throughput measurement or speedup is claimed.

`akroma_family_public_v40::build_for_scope` constructs family parts. Most
standalone artifact identities are already omitted from these parts. In
particular, Juggernaut's `parts_for_common_scope` uses an empty producer string;
its standalone producer is also called by Akroma's already cached dependency
initializer. Hashing every uncached standalone producer is therefore not the
repeated cost of every complete input.

Four producers are still called directly by successful complete-family builds:

| Producer | Source entries | Source bytes | Framed bytes per call | Direct calls in one build |
| --- | ---: | ---: | ---: | ---: |
| `fiery_public_v1::producer_sha256_v1` | 10 | 3,430,065 | 3,430,075 | 3 |
| `krenko_public_v1::producer_sha256_v1` | 15 | 3,704,023 | 3,704,038 | 2 |
| `fishing_pole_public_v39::producer_sha256_v39` | 17 | 4,526,733 | 4,526,750 | 2 |
| `twinflame_observation_v1::producer_sha256_v1` | 52 | 6,940,129 | 6,940,181 | 1 |

Counts use the exact base-checkout UTF-8 bytes, with `SCHEMA` counted as one
entry and repeated include paths retained. Each entry has its original leading
zero-byte separator. These direct calls account for 33,691,982 framed bytes in
one successful base family build, before any other caller repeats it. This is
a static byte count, not a measured elapsed cost or a whole-input profile.

Fiery's parts call `facts_v1` through authority validation, payload construction
and `cast_menu`. Krenko calls `facts_v1` through authority validation and payload
construction. Fishing calls `facts_v39` through the same two paths. Twinflame's
parts call `observe_inner_scoped`, which computes the producer before parts
clear the standalone identity fields. Empty current facts still construct
their producer identity. Separate artifact validation can also request these
same identities.

Each of these four no-argument functions reads only its literal `SCHEMA` and
ordered `include_str!` list. The patch adds a private `OnceLock<String>` around
each entire original body and returns a cloned string. The source lists,
duplicates, separator framing, SHA-256 operations and hexadecimal formatting
remain unchanged. Their new source bytes naturally produce new identities;
cached and original algorithms agree for the same compiled input bytes.

The other 18 uncached producers in these family modules are unchanged: the 17
legacy standalone observation producers from Sower through Strongbox, plus
Fishing's standalone observation wrapper. Their parts omit these identities.
The five already cached Fiery/Krenko/Zul/Banner/Inspiration observation wrappers
are unchanged. Existing Akroma, V51/V52/V57 and rules-meaning producer caches
remain intact. The broader 22-function inventory has 120,654,119 source bytes
per hypothetical one-call-each sweep; that is not the work of a complete input.

No state, snapshot, root, proof, world validation, projection commitment or
admission decision is cached. Existing assertions, fixture names and guards
remain unchanged. Static verification unwraps each initializer and checks that
its original body and all surrounding code match the base, then checks the
source list and framing. Pinned formatting and diff checks accompany the
change. The next performance decision still needs actual profiling/timing of
the current qualified path; this patch does not identify the cause of all
previous cases that ran for nearly 1800 seconds.
