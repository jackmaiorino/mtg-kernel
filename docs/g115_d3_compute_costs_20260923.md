# D3 cloud cost accounting

All 11 D3 cloud allocations have independent and controller release confirmations. Their quoted compute rates multiplied by observed allocation windows total **$4.8133** for **5.0139 pod-hours**. Using the wider lease-preparation-to-release-confirmation windows gives **$5.0064**. These are timing-based estimates, not provider invoices or strict billing bounds. Actual billed cost is unavailable from these receipts. The valid formal attempt is running locally.

| Allocation | Observed pod-hours | Quote x observed window, USD | Quote x lease window, USD |
|---|---:|---:|---:|
| Qualification 001 | 0.02477 | 0.02378 | 0.07092 |
| Qualification 002 | 0.13191 | 0.12664 | 0.13082 |
| Qualification 003 | 0.14822 | 0.14229 | 0.21714 |
| Qualification 004 | 1.80669 | 1.73442 | 1.73771 |
| Qualification 005 | 2.53772 | 2.43621 | 2.43912 |
| Qualification 006 | 0.25761 | 0.24730 | 0.25368 |
| Formal cloud preflight 001 | 0.01302 | 0.01250 | 0.04110 |
| Formal environmental void 002 | 0.05642 | 0.05416 | 0.06276 |
| Formal cloud preflight 003 | 0.01003 | 0.00963 | 0.01787 |
| Formal cloud preflight 004 | 0.01762 | 0.01692 | 0.02078 |
| Formal cloud preflight 005 | 0.00985 | 0.00945 | 0.01446 |

Every rate was $0.96 per hour. Observed windows start at the recorded successful creation response and end at the later of the two release confirmations. Lease windows start before provisioning; both windows include release-check overhead. Persistent shared storage, local electricity and earlier V4/D2 cloud work are excluded. The $10 per-lease and $200 cumulative caps, including conservative carried reservations, are authority limits, not expenditures. Account-balance changes include other activity and cannot isolate this lane.

Reproducible input index and arithmetic: `E:/mtg-g115-lineage-20260923/d3-cloud-cost-summary-001.json`, SHA-256 `7f73ff309b706adf998c74d52ee0df79b601d0aee6d04ea277d8b5185fe1b05e`. It hashes every lease, creation receipt, controller completion and both release confirmations. The read-only calculation is `E:/mtg-g115-lineage-20260923/summarize-d3-cloud-costs.py`. All failed qualification attempts, incompatible preflights and the zero-result environmental void remain in this total and in the retained evidence. No strength outcomes were read for this accounting.
