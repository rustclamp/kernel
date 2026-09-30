<img src="https://docs.rustclamp.com/assets/rustclamp-logo.png" alt="RustClamp logo" width="160">

# rustclamp-kernel

Kernel component of [RustClamp](https://github.com/rustclamp/rustclamp):
composition, typed capability resolution and process validation over
[rustclamp-core](https://github.com/rustclamp/core). Depends only on Core, no
other external crates. Companion crate, not a standalone framework.

## Install

Not published to crates.io yet (`publish = false`). Depend on it from git, Rust 1.96.1+:

```toml
[dependencies]
rustclamp-kernel = { git = "https://github.com/rustclamp/kernel" }
```

## Example

Pick a `Clock` provider explicitly or let a single provider win
(see [`examples/02-resolution.rs`](examples/02-resolution.rs)):

```rust
use rustclamp_core::{Clock, ClockCapability, ModuleId, SystemClock};
use rustclamp_kernel::{Provision, Resolver};

const GREETER: ModuleId = ModuleId::new("example.greeter");
const SYSTEM_CLOCK: ModuleId = ModuleId::new("example.system-clock");

let system = SystemClock;
let providers = [Provision::<ClockCapability>::new(SYSTEM_CLOCK, &system as &dyn Clock)];
let clock = Resolver::resolve::<ClockCapability>(GREETER, &providers, None)?;
```

## Main API

- **Resolution:** `Resolver`, `Provision`, `QualifiedProvision`,
  `CapabilityRequirement`, `ProviderSelection`; optional and many cardinality,
  structured `CompositionError`.
- **Planning snapshots:** `CapabilityComposition` (`without_module`,
  `replace_provider`, `validate`), `ConstructionGraph` (cycle detection with
  closed module paths).
- **Contributions:** `TargetComposition<T, Q>` hands typed contributions to a
  Core `ContributionTarget`.
- **Process projection:** `ApplicationBlueprint` (`add_root`, `project`, `freeze`)
  resolves only what a process reaches, with inclusion and exclusion reasons.
  `freeze` consumes the blueprint into a `FrozenProcess`:
  - `resolve`, `resolve_shared` (owned `Arc` via `SharedProvision`),
    `resolve_with` (lazy `FactoryProvision`, fallible with `FactoryError`);
  - `compose` / `compose_inferred` build a contribution target from the frozen
    process (`ComposeError`).

Kernel does not construct modules or manage lifecycle; that is
[runtime](https://github.com/rustclamp/runtime). Feature flags: none. Benchmarks:
`cargo bench` (`resolution`, `process_projection`); measurements are in the
[facade evidence](https://github.com/rustclamp/rustclamp/tree/main/docs/evidence).
See [CHANGELOG.md](CHANGELOG.md).

## Documentation

<https://docs.rustclamp.com>

## Development

```sh
cargo fmt --all -- --check
cargo clippy --offline --locked --all-targets --all-features -- -D warnings
cargo test --offline --locked --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --offline --locked --no-deps --all-features
```

Coordinated checkout, architecture checks and release policy: see the
[facade contributor guide](https://github.com/rustclamp/rustclamp/blob/main/CONTRIBUTING.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Unless you state otherwise, any
contribution you submit for inclusion is dual licensed as above, without
additional terms or conditions.
