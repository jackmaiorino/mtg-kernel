# Parallel public collector qualification, September 20

PASS: 26 updates and 260 natural games across both public-input arms, 1/2/4/8 collectors and a fresh-process four-worker structured resume. Every saved checkpoint, full optimizer and episode file is byte-identical across worker counts. New serial also matches the first three updates of the preserved original pilot. Total qualification wall time 321.47 seconds.

| Arm | Workers | Three-update process seconds | Mean updates 1/2 seconds | Collection seconds, all three |
|---|---:|---:|---:|---:|
| control | 1 | 42.73 | 5.95 | 13.48 |
| control | 2 | 31.99 | 4.98 | 8.27 |
| control | 4 | 32.26 | 5.24 | 7.64 |
| control | 8 | 28.70 | 4.40 | 6.84 |
| structured | 1 | 46.45 | 10.08 | 22.00 |
| structured | 2 | 33.90 | 5.38 | 10.24 |
| structured | 4 | 28.87 | 3.29 | 6.71 |
| structured | 8 | 31.39 | 4.68 | 8.20 |

Best observed full-process improvement: control 42.73 to28.70 seconds (1.49x, eight workers); structured46.45 to28.87 (1.61x, four workers). These are small engineering timings, not a production allocation decision. Worker order was1/2/4/8; a separate owner's cargo/rustc test build started19:48:48 EDT and ran during later measurements. Native process startup and CUDA initialization are included. The two post-startup batches are too few to extrapolate a precise full-campaign speedup. No compute-choice.json was issued and no substantial continuation launched.

Native source600b02c52201b0f21cbad945cef6093312b4016b; executable SHA d21d34b972abe7e70ff686aa3674dc29569377fbbf5e9620f8a6d2a3d5efd940. Runner7821c84a. Root E:/mtg-meta-recovery-20260920/public-collector-qualification-002. Original root001 stopped before native execution because WDDM included the Codex desktop context in the compute list. Root002 narrowly accepts the installed Codex UI context only with <=16MiB and0% GPU utilization; unknown contexts remain blocked. No application was terminated.

Placement remains incomplete. HaleysPC was reached through SSH and has a Ryzen73700X8core/16thread CPU,32GB RAM, RTX4060GPU0; no native workload at inventory time. Current trainer validation requiresGPU1, so copying this config there is not a qualified training path. Jack's PC has i7-13700K16core/24thread CPU,128GB RAM, RTX4070Super12GB and RTX3050GPU1. A read-only authenticated RunPod inventory request returnedHTTP403; availability/balance were not verified and no rental was made. Existing completed cloud evidence remains untouched.

Next: confirm timing with repeated/order-balanced runs once competing work is clear, qualify compatible placement on the other eligible hardware, and migrate future supported launchers before substantial work. Preserve exact seeds, batch weights/order and optimizer updates. Current Python training launcher rejects absent/incompatible compute evidence, but raw/native and older launch paths are not universally enforced. Shared instruction edits do not prove existing sessions reloaded them. Fable remains zero-read HTTP429 review gap untilSeptember22 07:00EDT. Engineering parity and speed do not establish playing strength.
