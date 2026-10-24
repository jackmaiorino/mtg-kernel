# Future FDN CI coverage source checkpoint

Base: `ebf6094e5d9b906c97f9b35dc1cd52656ac00e5d`. This changes CI selection only. Native source remains uncompiled and unrun locally; no capability or profile changes are made.

The existing 16 model and Human shards already select their complete subtrees, including Call, EndTurn, Genesis and Vivien. Their commands, ignored-case policy, partitioning and assertions are unchanged. The original V1 through V7 protocol commands are also retained.

Four sibling matrix groups add actual V8 through V11 binaries and compiled Rust wire, sealed-owner, sampler, public-fact and native printing cases. Each selected module uses the existing compiled inventory and isolated-case runner: exact names, immediate panic output, 1800 seconds per case, one native test thread and at most four concurrent children (fewer when available CPU capacity is lower). A failed module is recorded without suppressing subsequent modules. The original 120-minute job bound and required aggregate checks remain.

| Group | Added source test count | Coverage |
|---|---:|---|
| Existing limited-v7 | 42 | Alesha/Nine native, full public facts/observation/candidates and sealed owner |
| New limited-v8 | 98 | Call V8 wire/owner/sampler; Call through Zimone native and public facts; Conqueror coexistence |
| New limited-v9 | 91 | EndTurn V9 wire/owner/sampler; Tutelage, Aberration, Lathril, Curator, Kykar, Greatwurm, Time Stop and common projection |
| New limited-v10 | 109 | Genesis V10 wire/owner/sampler; Electroduplicate/Harvester, Progenitus, Etali, Axe, Fiend, Moon, Storm and Genesis |
| New limited-v11 | 98 | Vivien V11 wire/owner, Muldrotha (17 groups), Loot, ordinary cleanup, wide-X/life, cast identity/cost, Sphinx, Omniscience and Drake |

Counts are static `#[test]` source counts, not compiled inventory or execution results. The compiled runner checks every module is nonempty and logs the actual inventory. New selections do not overlap the pre-existing model/Human shard selections. All 305 prior source-roster names and all 364 Native213 selected names are covered by the union of retained and new filters; the real Muldrotha module adds all 17 current groups.

The four added groups add four pull-request jobs (actual total 42 to 46) and eight jobs on full two-OS matrix events. They reuse the existing per-group Cargo build cache and one Limited library, rather than adding all-target builds. V8 also invokes its existing real-process checker over basic Full decks at seed 123, labelled plumbing. V9, V10 and V11 execute their actual Rust wire tests and build their declared binary; no process checker is invented for them.

The four new groups distribute roughly 100 source cases each instead of serializing the 438 new selections in one job. Retained Native206 current-base records show 48 cases consumed 4400.534 summed child-seconds, including 1081.453 seconds for the Hatcher model alone. That expensive model is already in the 16 shards and is not duplicated here. Retained Native213 early records are 33 model cases only, so they do not establish elapsed times for these new native groups. The allocation is source-balanced; hosted runtime and the 120-minute envelope still require actual observation. No timeout increase or runtime-pass claim is made.
