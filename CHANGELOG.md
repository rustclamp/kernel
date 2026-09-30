# Changelog: rustclamp-kernel

## Unreleased

### Added

- `FrozenProcess::compose_inferred::<T>` reads the qualifier from the
  blueprint's `add_contribution` edges instead of a `Q` turbofish; several or
  no qualifiers fail with the new `ComposeError::AmbiguousQualifier` (#5).
- `FrozenProcess::compose` builds a contribution target from the frozen
  process: out-of-process contributions are dropped, and supplied values must
  match the blueprint edges (new `ComposeError`).
- `FrozenProcess::resolve` returns a capability value through the provider the
  frozen process selected, so the blueprint is the only declaration of an edge
  (ADR 0017). New `CompositionErrorKind::UndeclaredRequirement`.
- `ApplicationBlueprint::add_root` declares a single-root process in one call.
- `Display` and `Error` for `ProjectionError` and `TargetCompositionError`.
- Typed single-capability resolution with explicit selection and structured errors.
- Typed qualifiers, optional/many cardinality, and caller-owned composition
  snapshots with distinct module removal and provider replacement operations.
- Caller-owned construction dependency graphs with deterministic, structured
  cycle paths.
- Caller-owned typed contribution collections that delegate build semantics to
  Core's public contribution-target contract and report required orphans.
- Caller-owned application blueprints that derive module reachability from
  process execution roots, capability-provider links, and qualified targets.
- Projection-scoped provider discovery/default selection, replacements,
  exclusions, cycle/orphan diagnostics, and provenance for resolved edges.
- Consuming process freeze into separate runtime and inspection views, with
  selected requirements, contributions, inclusion paths, and exclusion reasons.
- Adapters from additive Core module, requirement, and provision contracts to
  the typed resolver declarations.
- Phase 0 package scaffold and development checks.
- Expanded the package README with the intended resolution flow, contribution
  boundary, and current scaffold status.
