//! Exercises typed capability resolution and structured failure diagnostics.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustclamp_core::{Capability, CapabilityId, Clock, ClockCapability, ModuleId};
use rustclamp_kernel::{CompositionError, CompositionErrorKind, Provision, Resolver};

const GREETER: ModuleId = ModuleId::new("test.greeter");
const SYSTEM_CLOCK: ModuleId = ModuleId::new("test.system-clock");
const FIXED_CLOCK: ModuleId = ModuleId::new("test.fixed-clock");

fn error<T>(result: Result<T, CompositionError>) -> CompositionError {
    match result {
        Ok(_) => panic!("expected composition to fail"),
        Err(error) => error,
    }
}

struct FixedClock(SystemTime);

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

#[test]
fn one_provision_resolves_to_its_typed_value() {
    let clock = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let provisions = [Provision::<ClockCapability>::new(
        FIXED_CLOCK,
        &clock as &dyn Clock,
    )];

    let resolved = Resolver::resolve::<ClockCapability>(GREETER, &provisions, None).unwrap();
    assert_eq!(resolved.now(), UNIX_EPOCH + Duration::from_secs(42));
}

#[test]
fn missing_provider_error_preserves_structured_identities() {
    let error = error(Resolver::resolve::<ClockCapability>(GREETER, &[], None));

    assert_eq!(error.kind(), CompositionErrorKind::MissingProvider);
    assert_eq!(error.required_by(), GREETER);
    assert_eq!(error.capability(), ClockCapability::ID);
    assert_eq!(
        error.capability(),
        CapabilityId::new("rustclamp.core.clock")
    );
    assert!(error.candidates().is_empty());
}

#[test]
fn ambiguity_is_independent_of_registration_order() {
    let system = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let fixed = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let first_order = [
        Provision::<ClockCapability>::new(SYSTEM_CLOCK, &system as &dyn Clock),
        Provision::<ClockCapability>::new(FIXED_CLOCK, &fixed as &dyn Clock),
    ];
    let reverse_order = [
        Provision::<ClockCapability>::new(FIXED_CLOCK, &fixed as &dyn Clock),
        Provision::<ClockCapability>::new(SYSTEM_CLOCK, &system as &dyn Clock),
    ];

    let first = error(Resolver::resolve::<ClockCapability>(
        GREETER,
        &first_order,
        None,
    ));
    let reverse = error(Resolver::resolve::<ClockCapability>(
        GREETER,
        &reverse_order,
        None,
    ));

    assert_eq!(first, reverse);
    assert_eq!(first.kind(), CompositionErrorKind::AmbiguousProviders);
    assert_eq!(first.candidates(), &[FIXED_CLOCK, SYSTEM_CLOCK]);
}

#[test]
fn explicit_selection_replaces_the_unique_default_without_order_dependence() {
    let system = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let fixed = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let provisions = [
        Provision::<ClockCapability>::new(SYSTEM_CLOCK, &system as &dyn Clock),
        Provision::<ClockCapability>::new(FIXED_CLOCK, &fixed as &dyn Clock),
    ];

    let resolved =
        Resolver::resolve::<ClockCapability>(GREETER, &provisions, Some(FIXED_CLOCK)).unwrap();
    assert_eq!(resolved.now(), UNIX_EPOCH + Duration::from_secs(42));
}

#[test]
fn unavailable_selection_is_a_structured_error() {
    let system = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let provisions = [Provision::<ClockCapability>::new(
        SYSTEM_CLOCK,
        &system as &dyn Clock,
    )];

    let error = error(Resolver::resolve::<ClockCapability>(
        GREETER,
        &provisions,
        Some(FIXED_CLOCK),
    ));

    assert_eq!(
        error.kind(),
        CompositionErrorKind::ProviderSelectionUnavailable
    );
    assert_eq!(error.candidates(), &[SYSTEM_CLOCK]);
}

#[test]
fn duplicate_identity_cannot_make_selection_order_dependent() {
    let first = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let second = FixedClock(UNIX_EPOCH + Duration::from_secs(2));
    let provisions = [
        Provision::<ClockCapability>::new(FIXED_CLOCK, &first as &dyn Clock),
        Provision::<ClockCapability>::new(FIXED_CLOCK, &second as &dyn Clock),
    ];

    let error = error(Resolver::resolve::<ClockCapability>(
        GREETER,
        &provisions,
        Some(FIXED_CLOCK),
    ));

    assert_eq!(error.kind(), CompositionErrorKind::AmbiguousProviders);
    assert_eq!(error.candidates(), &[FIXED_CLOCK, FIXED_CLOCK]);
}
