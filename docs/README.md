# Documentation

![Native Rust level editor](images/native-level-editor.png)

Use these documents according to what you are trying to do:

| Document | Purpose |
| --- | --- |
| [Illustrated usage guide](USAGE.md) | Open a ROM, navigate, edit, save, and capture rendering diagnostics |
| [Feature-parity ledger](FEATURE_PARITY_MATRIX.md) | Product workflows, evidence gates, and remaining parity gaps |
| [Architecture](REIMPLEMENTATION_ARCHITECTURE.md) | Crate boundaries, dependency direction, and clean-room design |
| [Compatibility test matrix](REIMPLEMENTATION_TEST_MATRIX.md) | Required format, transaction, GUI, oracle, and variant coverage |
| [Implementation notes](IMPLEMENTATION_NOTES.md) | Detailed implementation decisions and recovered subsystem behavior |
| [Reverse-engineering ledger](REVERSE_ENGINEERING.md) | Ghidra addresses, Wine observations, confidence, and ground truth |

Retained oracle provenance:

- [Credits transfer](oracle-work/lm363/pristine-us/credits-transfer-positive/PROVENANCE.md)
- [Legacy level 105 transfer](oracle-work/lm363/pristine-us/legacy-level-105/PROVENANCE.md)
- [Overworld transfer](oracle-work/lm363/pristine-us/overworld-transfer-positive/PROVENANCE.md)
- [Title-screen transfer](oracle-work/lm363/pristine-us/title-screen-transfer-positive/PROVENANCE.md)

Return to the [project README](../README.md) for installation, build commands, current capabilities,
and contributor guidance.
