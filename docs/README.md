# Documentation

Research documentation is organized by role:

- `contracts/`: frozen specifications and execution manifests
- `reports/`: completed experiment and validation results
- `design/`: design drafts
- `archive/`: superseded or invalid historical records

Some historical records include filenames and SHA-256 values captured when the files
were first committed at the repository root. Those files remain byte-identical after
relocation so the recorded hashes stay valid. Audit tooling distinguishes their
current documentation paths from their original Git paths.

## Start here

- [Build, interfaces and verification](../README.md)
- [Science-ready acceptance roadmap](../ROADMAP.md)
- [Standalone extraction provenance](archive/EXTRACTION_PROVENANCE.md)

## Game interfaces and accepted support

- [Custom-deck JSONL interface and Python adapter](design/limited_custom_session_v1.md)
- [Engine priority windows, schema 2](design/limited_priority_windows_v1.md)
- [Foundations combat damage, schema 3](design/fdn_combat_damage_v1.md)
- [London mulligans, schema 4](design/fdn_london_mulligans_v1.md) and [validation](reports/fdn_london_mulligans_v1_validation.md)
- [Foundations scope and implementation milestones](design/fdn_limited_implementation_v1.md); full coverage remains tracked in [issue #110](https://github.com/jackmaiorino/mtg-kernel/issues/110)
- [MageZero Standard fixture catalog](../data/standard/magezero_v1/README.md) and [engineering completion](reports/standard_completion_v1.md)

## Policy and research contracts

These records describe their named versions and scopes. Historical status text
does not grant execution authority or establish current acceptance.

- [Flat policy V1 design](design/FLAT_POLICY_V1_DESIGN.md) and [validation](reports/FLAT_POLICY_V1_VALIDATION.md)
- [Common model snapshot contract](contracts/COMMON_MODEL_SNAPSHOT_V1.md)
- [Native trainer schedule](contracts/NATIVE_TRAINER_SCHEDULE_V1.md)
- [Phase 1 Pauper meta plan](phase1_pauper_meta_competence.md)
- [Cycle-4 arm launcher](native_cycle4_arm_launcher_v1.md), [refresh manifest](native_population_refresh_manifest_cycle4_v1.md), and [routing compatibility](cycle4_routing_compatibility_v1.md)

## Hash-pinned diagnostics

- [Action-ingress admission v1](contracts/ACTION_INGRESS_ADMISSION_V1.md)
- [Action-ingress admission v2](contracts/ACTION_INGRESS_ADMISSION_V2.md)
- [Observation diagnostics execution v2](contracts/OBSERVATION_DIAGNOSTICS_EXECUTION_V2.md)
- [Observation diagnostics classification retry v1](contracts/OBSERVATION_DIAGNOSTICS_CLASSIFICATION_RETRY_V1.md)
- [Observation diagnostics result v1](reports/OBSERVATION_DIAGNOSTICS_RESULT_V1.md)
- [Invalid action-ingress execution v1](archive/ACTION_INGRESS_ADMISSION_EXECUTION_V1_INVALID.md)
