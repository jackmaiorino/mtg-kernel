# D3 qualification, September 23

Current launch-source status, 10:38 EDT: `cd41885e` has completed its local serial reference in **5,119.382 seconds**. All 64 stores were independently rehashed and validated through the native reader: 150 natural games, 30,447 decisions, the frozen g115/V3 identities, exact requests and source commit, natural endings and search records. Every store has the same semantics as reviewed `e258daf3` after only the declared V3 envelope build-provenance normalization. Receipt: `E:/mtg-g115-lineage-20260923/d3-reviewed-local-serial-003-verified.json`; phase completion SHA-256 `61fa37a556d5e5cea9979fac682fc2cbd7e8c7195eed123572c6fe7129c91d46`.

At 10:58 EDT, the current-source eight-worker phase completed in **1,264.209 seconds**, a **4.0495x** speedup over serial including its entire final slow-match tail. All 64 stores were independently rehashed and byte-matched against serial, with native source/input/completion checks: again 150 natural games and 30,447 decisions. Receipt: `E:/mtg-g115-lineage-20260923/d3-reviewed-local-workers8-003-verified.json`; phase completion SHA-256 `1dd55ae10ce14c3df27423badf015fba61d57c5d69ef08fe87ca1fd842a14180`.

The local 16/24-worker comparisons and the cloud and Haley grids remain active. No current-source fleet qualification or fastest-allocation claim is complete. Current timings include the actual host load; Kimi #029 reports concurrent workload contention and has made its retry skip D3-controller windows. A fresh resource inventory is still required before formal dispatch. The old completed grid below is preserved as historical evidence and is not relabeled as the current build or current cloud hardware. No formal D3 match has launched.

**Complete, 04:37 EDT.** Both full scaling grids passed. Every recovered cloud store was independently rehashed and matched the local serial reference: 256 cloud stores plus the earlier 256 local stores, covering the same 64 distinct matches in each phase. Each phase has 150 natural games and 30,447 decisions. Repetitions establish engineering parity, not independent strength evidence. No formal shared-panel match has launched.

| Workers | Local seconds | Cloud seconds |
| ---: | ---: | ---: |
| 1 | 4310.398 | 4234.596 |
| 8 | 954.291 | 975.729 |
| 16 | 720.932 | 675.472 |
| 24 | 629.300 | Not measured |
| 32 | Not measured | 526.088 |

Measured fastest: local 24 workers, 6.850x serial; cloud 32 workers, 8.049x serial. Cloud hardware was EPYC9655, 32 logical / 16 physical cores, 64 GB cgroup limit, fuse-backed workspace. The formal cloud controller rejects a materially incompatible replacement before native dispatch. The earlier EPYC9654 allocation is not interchangeable evidence.

Cloud qualification completed in 6411.974 seconds; full controller turnaround was 6504.053 seconds. Measured setup, transfer, observation, recovery and release overhead totals 92.079 seconds, including 20.765 seconds recovering the 11,085,706-byte archive. Pod r8l0yfu513o8w7 was released at 04:36:05 EDT, confirmed independently by both guards. Fresh browser-User-Agent inventory returned HTTP200 with zero Pods. All owned qualification workers are finished, and volume m90klwkv15 is retained. The last conservative guard forecast was $3.2834 for this lease and remained below its $10 increment / $200 total caps; this is accounting, not a billing statement.

Receipts: E:/mtg-g115-lineage-20260923/d3-cloud-qualification-004-verified.json and d3-local-qualification-002-verified.json. Full cloud root: D:/g115-d3-cloud-control/d3-cloud-qualification-004. Qualification completion SHA256 7e5446b465c8222b85df3860872396c608b8dc4662b991f85161f52b78956fee; recovered archive SHA256 90a4b033442ad585ff4656b929bf3016cfd28b765b5bff69bd21edd0b732b561. Native source remains e258daf3ab807cd6d8616a1431a21ea5ee22ac0b.

Outcome-independent allocation projection for all 2,048 formal jobs, preserving four-job seed clusters:

| Allocation | Jobs local / cloud | Projected hours | Cloud compute cost |
| --- | ---: | ---: | ---: |
| Local only | 2048 / 0 | 5.594 | $0 |
| Cloud only | 0 / 2048 | 4.702 | $4.51 |
| Local plus cloud | 936 / 1112 | 2.565 | $2.46 |

The combined allocation is fastest under the measured rates and overhead. Evidence: E:/mtg-g115-lineage-20260923/d3-allocation-projection-001.json, using the supported launcher's existing cluster_allocation function. Costs exclude storage and uncertainty margins. The 64-case cohort is representative timing, not an upper bound on the formal panel. A future lease must explicitly cover its chosen shard timeout, staging and recovery, within existing caps. Larger formal payload transfer/recovery can exceed this engineering measurement. Different future hardware requires requalification and a new projection including that cost and delay. No Pod is being retained during the review wait.

Fresh three-host inventory: D:/g115-d3-qualification-20260923/host-inventory-005.json. Jack's PC is eligible with about85.45 GiB free RAM; preserve the 32 GiB user reserve. HaleysPC SSH remains unreachable at 100.71.75.65. RunPod is available through the corrected User-Agent, with zero current Pods. Refresh inventory and competing reservations immediately before dispatch; this snapshot is not standing availability.

The supported formal paths are g115_d3_launch_v1.py and g115_d3_cloud_v1.py. Plan validation precedes paid allocation, and actual worker validation requires review disposition, complete qualification, compatible runtime/hardware and current lease/resource checks. The absent design verdict is still rejected. Positive formal dispatch remains untested. Final manifest/support packaging still needs the actual design disposition, refreshed placement/lease, collector pin and current seven worker modules. No new infrastructure or qualification campaign is needed merely to occupy the review wait.

Historical progression below preserves failed attempts and superseded intermediate states. It is not current launch authority.


Status at04:17 EDT: local qualification complete; cloud qualification still running. No formal shared-panel match has launched. No playing-strength verdict is available.

Completed local measurements; each row covers the same64 matches and all stores match the serial reference exactly:

| Workers | Seconds | Speedup versus serial |
| ---: | ---: | ---: |
| 1 | 4310.398 | 1.000x |
| 8 | 954.291 | 4.517x |
| 16 | 720.932 | 5.979x |
| 24 | 629.300 | 6.850x |

All256 local native stores were independently rehashed and their completion counts checked in E:/mtg-g115-lineage-20260923/d3-local-qualification-002-verified.json. Each phase contains150 natural games and30,447 decisions. The complete process exited0 after6614.964 seconds;24 workers was fastest on this measured cohort. Aggregate D:/g115-d3-qualification-20260923/local-002/completion.json SHA2569fbdb792e6ba4fe5ab3942344dd7c3414af6d18d6872c595ba7c4ff5d0f5cca0. No native workers remain from this local qualification.

RunPod8 workers completed975.729 seconds,4.33993x its4234.596-second serial reference. All64 native stores were independently rehashed remotely and match the full serial/local map; receipts D:/g115-d3-cloud-control/d3-cloud-qualification-004/workers8-phase-verified.json and workers8-check.json. Cloud16-worker qualification is active,32 workers follows, then recovery and release. No final fleet allocation is selected. All repetitions are engineering measurements, not independent strength samples.

Current checkpoint, September23 03:38 EDT: repaired native e258daf3 completed the full64-case local serial pass in4310.398 seconds (0.0148478 BO3/s). All64 completed store hashes were independently verified, covering150 natural games and30,447 decisions. Evidence: D:/g115-d3-qualification-20260923/local-002/workers-1/completion.json and E:/mtg-g115-lineage-20260923/d3-local-serial-002-check.json. The eight-worker pass is active;16/24-worker phases follow. RunPod004 remains in its serial phase under the existing lease. No whole-host qualification or fastest allocation is established until the complete comparisons pass. Historical failure and preparation notes below remain preserved. The formal launcher preparation now exists at1b4a8297, but positive formal dispatch is still untested and the design verdict remains pending.

Update03:54 EDT: the complete eight-worker pass took954.291 seconds, a4.51686x completed-work speedup over the4310.398-second serial reference. All64 output hashes were independently recomputed and match the serial stores exactly. Receipt E:/mtg-g115-lineage-20260923/d3-local-workers8-002-check.json. The measured time includes the slow final match; no case or delay was dropped. Local16-worker qualification is active, with24 workers still to follow; no final worker count or fleet allocation is selected. RunPod has61/64 serial matches completed. Refreshed inventory D:/g115-d3-qualification-20260923/host-inventory-004.json still finds HaleysPC SSH unreachable, Jack eligible and RunPod HTTP200 with browser User-Agent, showing only the existing guarded004 Pod. No new allocation or formal measurement.

Update04:00 EDT: RunPod004 completed all64 serial matches in4234.596 seconds. A read-only SSH check independently rehashed all64 remote native stores and verified their completion records, then matched the full ID/hash map against the local serial reference: all exact,150 natural games and30,447 decisions. Receipts D:/g115-d3-cloud-control/d3-cloud-qualification-004/serial-phase-verified.json and serial-cross-host-check.json. The cloud serial reference is1.01790x local, about1.8% faster. This is not the final allocation comparison: cloud8/16/32-worker phases, local16/24 phases and cloud recovery/release remain. No strength outcomes were analyzed, and all qualification seeds remain disjoint from the formal panel.

The guarded worker (`python/tools/g115_d3_qualify_v1.py`, cf4be252) fixes 32 consumed BO3 cases, baseline and search in both seats across all eight decks, disjoint from the 512 formal seed clusters. Serial then increasing worker counts must reproduce exact semantic store bytes. Local dispatch preserves 32 GiB free RAM; cloud dispatch requires the named resident lease guard before and during work. A fresh three-host inventory is required. The guarded formal launcher is not yet implemented.

Local source6604306c completed 11 cases, then failed in `pair-2-3-p1-search`: g115 Elves in seat1 versus frozen V3 Faeries, seed1904492179334205846, game1, environment seed16851034818384397475, physical decision152/step190, five legal actions. Error: `State { stage: Redeterminize, source: HiddenStateContract }`. The 39.869-second failed case and every successful prefix remain at `D:/g115-d3-qualification-20260923/local-001`. The full attempt took713.385seconds. The aggregate stop error does not supersede the concrete native failure sidecar.

Cloud attempt001 failed before any match because its SSH worker environment omitted the exact pod identity. Attempt002 explicitly transports that identity and retains the strict check. Both attempts have separate, preserved receipts. Attempt002 completed seven cases with exact local semantic hashes, then was stopped cooperatively after the local failure was discovered. Recovery completed, and the provider confirmed both owned pods absent. Nothing on the persistent volume was deleted. Evidence: `E:/mtg-g115-lineage-20260923/d3-cloud-qualification-001` and `d3-cloud-qualification-002`, including each `release-confirmed.json` and `external-guard-completion.json`.

The cloud quote was cpu3c32, $0.96/hour, existing 10GB EU-RO-1 volume. Each lease had a three-hour deadline, $10 increment cap, $200 total cap, startup/work-idle/recovery limits and an independent local guard plus resident guard. Projected conservative lease cost was $5.0376. Historical allocations were conservatively charged their full caps in the authority ledger; no cap was reset between attempts. Approximate pod time was90seconds and468seconds respectively; this is not a final billing statement. Browser User-Agent correction was applied to the nested creation/guard APIs before use. The external guard's startup-idle logic was corrected to hand off activity checks to the resident guard once the worker had started; receipts preserve the first helper and its replacement.

HaleysPC SSH timed out in the fresh inventory. The cloud host reported AMD EPYC9654 and32 allowed CPUs, but its cgroup-v1 quota still needs explicit inspection on any future allocation. Incomplete serial samples do not establish fastest allocation, parallel scaling, or a cloud speedup. No further paid qualification is needed until the native correctness failure is diagnosed and repaired locally.

Prepared formal bindings at `D:/g115-d3-formal-binding-20260923` contain all2048 requests but are explicitly unlaunchable. They remain tied to source6604306c and must be regenerated after any native correction. Fixed models, search budget, panel, power statement and statistical gates are unchanged. The pending design review stays in the same FABLE-QUEUE entry. The failure is neither NO-ADVANCE nor permission to skip a case, substitute baseline play, or relax an information boundary.

Next qualification cohort correction, prepared before another run: add each deck's second cyclic opponent, keeping every original case. This gives64 cases with32 expensive search jobs and32 baseline jobs. The old cohort had only16 search jobs and therefore could not exercise32 concurrent search workers. Both serial and parallel measurements will use the same expanded cohort; this is a workload-sizing correction, not selection on winners. Incomplete attempts remain retained. The worker now also respects cgroup-v1/v2 memory limits and recovers per-job receipts into the aggregate report when a queued future raises after another case fails. No new qualification has launched.

Repaired-source qualification plan: native e258daf3; identical64-case cohort at workers1,8,16,24 on Jack's24-logical-CPU host and1,8,16,32 on a32-vCPU cloud host. Serial work is the required timing reference, with no strength gate. Earlier six completed search cases averaged approximately144seconds locally, implying about77minutes for32 serial search jobs and about95minutes for this local scaling grid; the unobserved cases may differ. The total qualification bound is9000seconds, individual BO3 bound1800seconds. A cloud attempt must fit its independent three-hour lease including startup and recovery; the lease always wins over the worker timeout. This changes neither the S128/T1024/depth8 search budget nor formal panel/gates. A fresh inventory at06:24UTC finds Jack's i7-13700K/16cores/24threads with about85GiB available, idle GPUs, and the preserved stopped human process; Haley SSH times out; RunPod browser-User-Agent inventory returns200/noPods. Local work uses the D SSD and BelowNormal priority. No fleet speedup is established yet.
