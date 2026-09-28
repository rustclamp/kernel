//! Exercises typed capability resolution and structured failure diagnostics.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustclamp_core::{
    Capability, CapabilityId, Clock, ClockCapability, Module, ModuleId, Provides, Qualifier,
    QualifierId, Requires,
};
use rustclamp_kernel::{
    CapabilityComposition, CapabilityRequirement, CompositionError, CompositionErrorKind,
    ConstructionDependency, ConstructionGraph, Provision, QualifiedProvision, Resolver,
};

const GREETER: ModuleId = ModuleId::new("test.greeter");
const SYSTEM_CLOCK: ModuleId = ModuleId::new("test.system-clock");
const FIXED_CLOCK: ModuleId = ModuleId::new("test.fixed-clock");
const A: ModuleId = ModuleId::new("test.a");
const B: ModuleId = ModuleId::new("test.b");
const C: ModuleId = ModuleId::new("test.c");
const DECLARED_CLOCK: ModuleId = ModuleId::new("test.declared-clock");
const DECLARED_GREETER: ModuleId = ModuleId::new("test.declared-greeter");

struct Primary;
struct Simulation;

impl Qualifier for Primary {
    const ID: QualifierId = QualifierId::new("test.primary");
}

impl Qualifier for Simulation {
    const ID: QualifierId = QualifierId::new("test.simulation");
}

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

struct ClockModule(FixedClock);

impl Module for ClockModule {
    const ID: ModuleId = DECLARED_CLOCK;
}

impl Provides<ClockCapability> for ClockModule {
    fn provided_value(&self) -> &(dyn Clock + 'static) {
        &self.0 as &(dyn Clock + 'static)
    }
}

struct GreeterModule;

impl Module for GreeterModule {
    const ID: ModuleId = DECLARED_GREETER;
}

impl Requires<ClockCapability> for GreeterModule {
    const SELECTED_PROVIDER: Option<ModuleId> = Some(DECLARED_CLOCK);
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
fn additive_module_contracts_discover_requirement_and_provider_declarations() {
    let system = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let selected = ClockModule(FixedClock(UNIX_EPOCH + Duration::from_secs(42)));
    let provisions = [
        Provision::<ClockCapability>::new(SYSTEM_CLOCK, &system as &dyn Clock),
        Provision::<ClockCapability>::from_module(&selected),
    ];
    let requirement = CapabilityRequirement::<ClockCapability>::from_module::<GreeterModule>();
    let resolved = Resolver::resolve::<ClockCapability>(
        requirement.required_by(),
        &provisions,
        requirement.selected_provider(),
    )
    .unwrap();

    assert_eq!(requirement.required_by(), DECLARED_GREETER);
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

#[test]
fn qualified_clocks_resolve_independently_with_typed_markers() {
    let primary_clock = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let simulation_clock = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let primary = [QualifiedProvision::<ClockCapability, Primary>::new(
        SYSTEM_CLOCK,
        &primary_clock as &dyn Clock,
    )];
    let simulation = [QualifiedProvision::<ClockCapability, Simulation>::new(
        FIXED_CLOCK,
        &simulation_clock as &dyn Clock,
    )];

    let primary_value =
        Resolver::resolve_qualified::<ClockCapability, Primary>(GREETER, &primary, None).unwrap();
    let simulation_value =
        Resolver::resolve_qualified::<ClockCapability, Simulation>(GREETER, &simulation, None)
            .unwrap();

    assert_eq!(primary_value.now(), UNIX_EPOCH + Duration::from_secs(1));
    assert_eq!(simulation_value.now(), UNIX_EPOCH + Duration::from_secs(42));
}

#[test]
fn qualified_missing_error_preserves_qualifier_identity() {
    let error = error(Resolver::resolve_qualified::<ClockCapability, Simulation>(
        GREETER,
        &[],
        None,
    ));

    assert_eq!(error.kind(), CompositionErrorKind::MissingProvider);
    assert_eq!(error.capability(), ClockCapability::ID);
    assert_eq!(error.qualifier(), Some(Simulation::ID));
    assert!(error.to_string().contains("test.simulation"));
}

#[test]
fn optional_requirement_is_absent_unique_or_ambiguous_without_a_default() {
    let system = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let fixed = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let none: [Provision<'_, ClockCapability>; 0] = [];
    let one = [Provision::<ClockCapability>::new(
        SYSTEM_CLOCK,
        &system as &dyn Clock,
    )];
    let two = [
        Provision::<ClockCapability>::new(SYSTEM_CLOCK, &system as &dyn Clock),
        Provision::<ClockCapability>::new(FIXED_CLOCK, &fixed as &dyn Clock),
    ];

    assert!(
        Resolver::resolve_optional::<ClockCapability>(GREETER, &none)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        Resolver::resolve_optional::<ClockCapability>(GREETER, &one)
            .unwrap()
            .unwrap()
            .now(),
        UNIX_EPOCH + Duration::from_secs(1)
    );
    let error = error(Resolver::resolve_optional::<ClockCapability>(GREETER, &two));
    assert_eq!(error.kind(), CompositionErrorKind::AmbiguousProviders);
    assert_eq!(error.candidates(), &[FIXED_CLOCK, SYSTEM_CLOCK]);
}

#[test]
fn many_requirement_returns_all_values_in_stable_identity_order() {
    let system = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let fixed = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let provisions = [
        Provision::<ClockCapability>::new(SYSTEM_CLOCK, &system as &dyn Clock),
        Provision::<ClockCapability>::new(FIXED_CLOCK, &fixed as &dyn Clock),
    ];

    let resolved = Resolver::resolve_many(&provisions);

    assert_eq!(
        resolved.iter().map(Provision::module).collect::<Vec<_>>(),
        [FIXED_CLOCK, SYSTEM_CLOCK]
    );
    assert_eq!(
        resolved
            .iter()
            .map(|provision| provision.value().now())
            .collect::<Vec<_>>(),
        [
            UNIX_EPOCH + Duration::from_secs(42),
            UNIX_EPOCH + Duration::from_secs(1),
        ]
    );
}

#[test]
fn removing_a_required_provider_keeps_consumer_and_fails_revalidation() {
    let system = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let requirement = CapabilityRequirement::<ClockCapability>::new(GREETER, None);
    let module_requirement = CapabilityRequirement::<ClockCapability>::new(SYSTEM_CLOCK, None);
    let composition = CapabilityComposition::new(
        vec![Provision::<ClockCapability>::new(
            SYSTEM_CLOCK,
            &system as &dyn Clock,
        )],
        vec![requirement, module_requirement],
    );
    assert!(composition.validate().is_ok());

    let without_provider = composition.without_module(SYSTEM_CLOCK);
    assert_eq!(without_provider.requirements().len(), 1);
    assert_eq!(without_provider.requirements()[0].required_by(), GREETER);
    let error = error(without_provider.validate());

    assert_eq!(error.kind(), CompositionErrorKind::MissingProvider);
    assert_eq!(error.required_by(), GREETER);
}

#[test]
fn replacing_one_provider_preserves_and_revalidates_consumer_requirements() {
    let system = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let fixed = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let requirement = CapabilityRequirement::<ClockCapability>::new(GREETER, Some(SYSTEM_CLOCK));
    let composition = CapabilityComposition::new(
        vec![Provision::<ClockCapability>::new(
            SYSTEM_CLOCK,
            &system as &dyn Clock,
        )],
        vec![requirement],
    );

    let replaced = composition
        .replace_provider(
            SYSTEM_CLOCK,
            Provision::<ClockCapability>::new(FIXED_CLOCK, &fixed as &dyn Clock),
        )
        .expect("existing provider should be replaceable");

    assert!(replaced.validate().is_ok());
    assert_eq!(
        replaced
            .resolve_requirement(replaced.requirements()[0])
            .unwrap()
            .now(),
        UNIX_EPOCH + Duration::from_secs(42)
    );
    assert!(
        composition
            .replace_provider(
                ModuleId::new("test.not-a-provider"),
                Provision::<ClockCapability>::new(FIXED_CLOCK, &fixed as &dyn Clock),
            )
            .is_none()
    );
}

#[test]
fn provider_replacement_and_clock_values_are_isolated_between_compositions() {
    let first_original = FixedClock(UNIX_EPOCH + Duration::from_secs(1));
    let first_replacement = FixedClock(UNIX_EPOCH + Duration::from_secs(42));
    let second_clock = FixedClock(UNIX_EPOCH + Duration::from_secs(99));
    let first_provider = ModuleId::new("test.first.clock");
    let first_replacement_provider = ModuleId::new("test.first.replacement");
    let second_provider = ModuleId::new("test.second.clock");
    let first = CapabilityComposition::new(
        vec![Provision::<ClockCapability>::new(
            first_provider,
            &first_original as &dyn Clock,
        )],
        vec![CapabilityRequirement::new(
            ModuleId::new("test.first.greeter"),
            Some(first_provider),
        )],
    );
    let second = CapabilityComposition::new(
        vec![Provision::<ClockCapability>::new(
            second_provider,
            &second_clock as &dyn Clock,
        )],
        vec![CapabilityRequirement::new(
            ModuleId::new("test.second.greeter"),
            Some(second_provider),
        )],
    );
    let replaced_first = first
        .replace_provider(
            first_provider,
            Provision::<ClockCapability>::new(
                first_replacement_provider,
                &first_replacement as &dyn Clock,
            ),
        )
        .unwrap();

    let resolved = |composition: &CapabilityComposition<'_, ClockCapability>| {
        composition
            .resolve_requirement(composition.requirements()[0])
            .unwrap()
            .now()
    };
    assert_eq!(resolved(&first), UNIX_EPOCH + Duration::from_secs(1));
    assert_eq!(
        resolved(&replaced_first),
        UNIX_EPOCH + Duration::from_secs(42)
    );
    assert_eq!(resolved(&second), UNIX_EPOCH + Duration::from_secs(99));
}

#[test]
fn construction_graph_accepts_acyclic_dependencies() {
    let graph = ConstructionGraph::new(vec![
        ConstructionDependency::new(A, B),
        ConstructionDependency::new(B, C),
    ]);

    assert!(graph.validate().is_ok());
}

#[test]
fn construction_cycle_reports_the_complete_closed_dependency_path() {
    let graph = ConstructionGraph::new(vec![
        ConstructionDependency::new(A, B),
        ConstructionDependency::new(B, C),
        ConstructionDependency::new(C, A),
    ]);

    let cycle = graph.validate().unwrap_err();
    assert_eq!(cycle.path(), &[A, B, C, A]);
    assert_eq!(
        cycle.to_string(),
        "construction cycle: test.a -> test.b -> test.c -> test.a"
    );
}

#[test]
fn construction_cycle_diagnostic_is_registration_order_independent() {
    let forward = ConstructionGraph::new(vec![
        ConstructionDependency::new(A, B),
        ConstructionDependency::new(B, A),
        ConstructionDependency::new(C, A),
    ]);
    let reverse = ConstructionGraph::new(vec![
        ConstructionDependency::new(C, A),
        ConstructionDependency::new(B, A),
        ConstructionDependency::new(A, B),
    ]);

    assert_eq!(
        forward.validate().unwrap_err(),
        reverse.validate().unwrap_err()
    );
}

#[test]
fn construction_graph_reports_a_self_dependency() {
    let graph = ConstructionGraph::new(vec![ConstructionDependency::new(A, A)]);

    assert_eq!(graph.validate().unwrap_err().path(), &[A, A]);
}

#[test]
fn construction_graph_handles_a_deep_acyclic_chain() {
    const MODULES: usize = 2_048;
    let ids = (0..MODULES)
        .map(|index| ModuleId::new(Box::leak(format!("test.chain.{index}").into_boxed_str())))
        .collect::<Vec<_>>();
    let dependencies = ids
        .windows(2)
        .map(|pair| ConstructionDependency::new(pair[0], pair[1]))
        .collect();
    let graph = ConstructionGraph::new(dependencies);

    assert!(graph.validate().is_ok());
}
