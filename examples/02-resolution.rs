//! Resolves Greeter's Clock requirement from one provider or an explicit choice.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustclamp_core::{Clock, ClockCapability, ModuleId};
use rustclamp_kernel::{Provision, Resolver};

const GREETER: ModuleId = ModuleId::new("example.greeter");
const SYSTEM_CLOCK: ModuleId = ModuleId::new("example.system-clock");
const TEST_CLOCK: ModuleId = ModuleId::new("example.test-clock");

struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

struct FixedClock(SystemTime);

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

struct Greeter<'a> {
    clock: &'a dyn Clock,
}

impl<'a> Greeter<'a> {
    fn new(clock: &'a dyn Clock) -> Self {
        Self { clock }
    }

    fn greet(&self, name: &str) -> String {
        let seconds = self
            .clock
            .now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("Hello, {name}! (unix second {seconds})")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let system_clock = WallClock;
    let only_system_clock = [Provision::<ClockCapability>::new(
        SYSTEM_CLOCK,
        &system_clock as &dyn Clock,
    )];
    let clock = Resolver::resolve::<ClockCapability>(GREETER, &only_system_clock, None)?;
    println!("{}", Greeter::new(clock).greet("world"));

    let test_clock = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let both_clocks = [
        Provision::<ClockCapability>::new(SYSTEM_CLOCK, &system_clock as &dyn Clock),
        Provision::<ClockCapability>::new(TEST_CLOCK, &test_clock as &dyn Clock),
    ];
    let clock = Resolver::resolve::<ClockCapability>(GREETER, &both_clocks, Some(TEST_CLOCK))?;
    println!("{}", Greeter::new(clock).greet("Ada"));
    Ok(())
}
