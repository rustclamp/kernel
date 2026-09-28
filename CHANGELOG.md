# Changelog: rustclamp-kernel

## Unreleased

### Added

- Typed single-capability resolution with explicit selection and structured errors.
- Typed qualifiers, optional/many cardinality, and caller-owned composition
  snapshots with distinct module removal and provider replacement operations.
- Caller-owned construction dependency graphs with deterministic, structured
  cycle paths.
- Caller-owned typed contribution collections that delegate build semantics to
  Core's public contribution-target contract and report required orphans.
- Adapters from additive Core module, requirement, and provision contracts to
  the typed resolver declarations.
- Phase 0 package scaffold and development checks.
- Expanded the package README with the intended resolution flow, contribution
  boundary, and current scaffold status.
