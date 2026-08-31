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

## Hash-pinned diagnostics

- [Action-ingress admission v1](contracts/ACTION_INGRESS_ADMISSION_V1.md)
- [Action-ingress admission v2](contracts/ACTION_INGRESS_ADMISSION_V2.md)
- [Observation diagnostics execution v2](contracts/OBSERVATION_DIAGNOSTICS_EXECUTION_V2.md)
- [Observation diagnostics classification retry v1](contracts/OBSERVATION_DIAGNOSTICS_CLASSIFICATION_RETRY_V1.md)
- [Observation diagnostics result v1](reports/OBSERVATION_DIAGNOSTICS_RESULT_V1.md)
- [Invalid action-ingress execution v1](archive/ACTION_INGRESS_ADMISSION_EXECUTION_V1_INVALID.md)
