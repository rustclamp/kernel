//! Dependency-free scaling benchmark for process-root projection.

use std::hint::black_box;
use std::time::Instant;

use rustclamp_core::{ApplicationId, CapabilityId, ExecutionId, ModuleId, ProcessId};
use rustclamp_kernel::ApplicationBlueprint;

const APP: ApplicationId = ApplicationId::new("phase4.benchmark.application");
const PROCESS: ProcessId = ProcessId::new("phase4.benchmark.process");
const ROOT_EXECUTION: ExecutionId = ExecutionId::new("phase4.benchmark.root");
const SAMPLES: usize = 9;

fn module(index: usize) -> ModuleId {
    ModuleId::new(Box::leak(
        format!("phase4.benchmark.module.{index}").into_boxed_str(),
    ))
}

fn capability(index: usize) -> CapabilityId {
    CapabilityId::new(Box::leak(
        format!("phase4.benchmark.capability.{index}").into_boxed_str(),
    ))
}

fn composition(count: usize) -> ApplicationBlueprint {
    let mut app = ApplicationBlueprint::new(APP);
    for index in 0..count {
        app.add_module(module(index));
        if index + 1 < count {
            app.require_provider(module(index), capability(index), None, module(index + 1));
        }
    }
    app.add_execution(ROOT_EXECUTION, module(0))
        .add_process(PROCESS, vec![ROOT_EXECUTION]);
    app
}

fn main() {
    for count in [1, 5, 20, 50, 100, 500] {
        let app = composition(count);
        let iterations = match count {
            1 | 5 => 10_000,
            20 => 5_000,
            50 => 2_000,
            100 => 500,
            _ => 100,
        };
        for _ in 0..10 {
            black_box(app.project(PROCESS).expect("valid chain"));
        }
        let mut samples = (0..SAMPLES)
            .map(|_| {
                let start = Instant::now();
                for _ in 0..iterations {
                    black_box(app.project(PROCESS).expect("valid chain"));
                }
                start.elapsed().as_nanos() / iterations as u128
            })
            .collect::<Vec<_>>();
        samples.sort_unstable();
        println!(
            "✓ BENCH process_projection modules={count} median_ns={} samples_ns={samples:?}",
            samples[SAMPLES / 2]
        );
        println!(
            "  Intent: resolve one reachable chain through {count} declared modules; composition construction is outside the timed region."
        );
    }
}
