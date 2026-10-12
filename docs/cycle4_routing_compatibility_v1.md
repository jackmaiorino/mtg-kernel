# Cycle-4 routing compatibility

This table preserves the routing refusals previously listed in the root README.
It describes compatibility checks for the named Cycle-4 tools; it does not change
their contracts, frozen measurements or execution authority. See the
[arm launcher](native_cycle4_arm_launcher_v1.md) and
[refresh manifest](native_population_refresh_manifest_cycle4_v1.md) for context.

| Tool | Refusal |
| --- | --- |
| `cycle4_m3_audit_v1` | The current reference document is `mtg-kernel-cycle4-m3-reference/v2` and the current audit report is `mtg-kernel-cycle4-m3-audit/v2`. The decoders explicitly refuse either obsolete `/v1` identity before decoding the current layout. |
| `cycle4_routing_v1` | `--reference-document` is required. Each M3 report must bind its canonical SHA-256, run, tip checkpoint, exact update 1537 through 2048 window, audit-note SHA-256, reference statistic bits, and derived dispersion allowance bits. |
| `cycle4_routing_v1` | The published routing record is `mtg-kernel-cycle4-routing-record/v2`; its decoder explicitly refuses the obsolete `/v1` identity. |
| `cycle4_routing_v1` and the M3 reference decoder | The reference window must declare first update 1537, last update 2048, update count 512, and a count equal to `last - first + 1`. |
| `run_m2_common_root_panel_v1.py` | The immutable panel is published only by hard link from its complete staged file. An identical existing panel is a no-op, a different existing panel is refused, and unavailable or unsupported hard linking fails closed. |
