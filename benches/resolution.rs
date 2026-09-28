//! Small dependency-free microbenchmark for successful Clock resolution paths.

use std::hint::black_box;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustclamp_core::{Clock, ClockCapability, ModuleId, Qualifier, QualifierId};
use rustclamp_kernel::{
    CapabilityComposition, CapabilityRequirement, Provision, QualifiedProvision, Resolver,
};

const GREETER: ModuleId = ModuleId::new("bench.greeter");
const REPETITIONS: usize = 100_000;
const SAMPLES: usize = 9;

struct Primary;

impl Qualifier for Primary {
    const ID: QualifierId = QualifierId::new("bench.primary");
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(42)
    }
}

fn sample(mut operation: impl FnMut()) -> u128 {
    let start = Instant::now();
    for _ in 0..REPETITIONS {
        operation();
    }
    start.elapsed().as_nanos() / REPETITIONS as u128
}

fn measure(name: &str, intent: &str, mut operation: impl FnMut()) {
    for _ in 0..2_000 {
        operation();
    }
    let mut samples = (0..SAMPLES)
        .map(|_| sample(&mut operation))
        .collect::<Vec<_>>();
    samples.sort_unstable();
    println!(
        "✓ BENCH {name}: median_ns={} samples_ns={samples:?}",
        samples[SAMPLES / 2]
    );
    println!("  Intent: {intent}");
}

fn main() {
    let no_provisions: [Provision<'_, ClockCapability>; 0] = [];
    let clocks = (0..8).map(|_| FixedClock).collect::<Vec<_>>();
    let values = clocks
        .iter()
        .map(|clock| clock as &dyn Clock)
        .collect::<Vec<_>>();
    let provisions = values
        .iter()
        .enumerate()
        .map(|(index, clock)| {
            let id = match index {
                0 => "bench.clock.0",
                1 => "bench.clock.1",
                2 => "bench.clock.2",
                3 => "bench.clock.3",
                4 => "bench.clock.4",
                5 => "bench.clock.5",
                6 => "bench.clock.6",
                _ => "bench.clock.7",
            };
            Provision::<ClockCapability>::new(ModuleId::new(id), *clock)
        })
        .collect::<Vec<_>>();
    let qualified_provisions = values
        .iter()
        .enumerate()
        .map(|(index, clock)| {
            let id = match index {
                0 => "bench.clock.0",
                1 => "bench.clock.1",
                2 => "bench.clock.2",
                3 => "bench.clock.3",
                4 => "bench.clock.4",
                5 => "bench.clock.5",
                6 => "bench.clock.6",
                _ => "bench.clock.7",
            };
            QualifiedProvision::<ClockCapability, Primary>::new(ModuleId::new(id), *clock)
        })
        .collect::<Vec<_>>();
    let graph_requirement = CapabilityRequirement::<ClockCapability>::new(
        GREETER,
        Some(ModuleId::new("bench.clock.0")),
    );
    let graph = CapabilityComposition::new(
        vec![Provision::<ClockCapability>::new(
            ModuleId::new("bench.clock.0"),
            values[0],
        )],
        vec![graph_requirement],
    );

    let direct = values[7];
    measure(
        "direct_typed_access",
        "Use the capability value directly as the no-resolver baseline.",
        || {
            black_box(black_box(direct).now());
        },
    );
    measure(
        "resolve_unique",
        "Find the only provider for a required Clock capability.",
        || {
            let resolved =
                Resolver::resolve::<ClockCapability>(GREETER, black_box(&provisions[..1]), None)
                    .unwrap();
            black_box(resolved.now());
        },
    );
    measure(
        "resolve_selected_2",
        "Select the last provider explicitly from two candidates.",
        || {
            let resolved = Resolver::resolve::<ClockCapability>(
                GREETER,
                black_box(&provisions[..2]),
                Some(ModuleId::new("bench.clock.1")),
            )
            .unwrap();
            black_box(resolved.now());
        },
    );
    measure(
        "resolve_selected_8",
        "Select the last provider explicitly from eight candidates.",
        || {
            let resolved = Resolver::resolve::<ClockCapability>(
                GREETER,
                black_box(&provisions[..]),
                Some(ModuleId::new("bench.clock.7")),
            )
            .unwrap();
            black_box(resolved.now());
        },
    );
    measure(
        "resolve_qualified_unique",
        "Resolve the only Clock provider under the Primary qualifier.",
        || {
            let resolved = Resolver::resolve_qualified::<ClockCapability, Primary>(
                GREETER,
                black_box(&qualified_provisions[..1]),
                None,
            )
            .unwrap();
            black_box(resolved.now());
        },
    );
    measure(
        "resolve_qualified_selected_8",
        "Select the last of eight Primary-qualified Clock providers.",
        || {
            let resolved = Resolver::resolve_qualified::<ClockCapability, Primary>(
                GREETER,
                black_box(&qualified_provisions[..]),
                Some(ModuleId::new("bench.clock.7")),
            )
            .unwrap();
            black_box(resolved.now());
        },
    );
    measure(
        "resolve_optional_absent",
        "Confirm an optional Clock requirement stays absent with no provider.",
        || {
            black_box(
                Resolver::resolve_optional::<ClockCapability>(GREETER, black_box(&no_provisions))
                    .unwrap(),
            );
        },
    );
    measure(
        "resolve_optional_unique",
        "Resolve an optional Clock requirement when one provider exists.",
        || {
            black_box(
                Resolver::resolve_optional::<ClockCapability>(GREETER, black_box(&provisions[..1]))
                    .unwrap(),
            );
        },
    );
    measure(
        "resolve_many_2",
        "Collect and identity-sort all providers for a many-valued requirement (two providers).",
        || {
            black_box(Resolver::resolve_many(black_box(&provisions[..2])));
        },
    );
    measure(
        "resolve_many_8",
        "Collect and identity-sort all providers for a many-valued requirement (eight providers).",
        || {
            black_box(Resolver::resolve_many(black_box(&provisions[..])));
        },
    );
    measure(
        "remove_module_and_revalidate",
        "Remove the required provider module and verify its consumer becomes invalid.",
        || {
            let updated = graph.without_module(ModuleId::new("bench.clock.0"));
            black_box(updated.validate().is_err());
        },
    );
    measure(
        "replace_provider_and_revalidate",
        "Replace one provider, retarget explicit selection, and validate the consumer.",
        || {
            let updated = graph
                .replace_provider(
                    ModuleId::new("bench.clock.0"),
                    Provision::<ClockCapability>::new(ModuleId::new("bench.clock.7"), values[7]),
                )
                .unwrap();
            black_box(updated.validate().is_ok());
        },
    );
}
