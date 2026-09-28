//! Tests for process-root reachability and provenance.

use rustclamp_core::{
    ApplicationId, CapabilityId, ContributionId, ContributionTargetId, ExecutionId, ModuleId,
    ProcessId, QualifierId,
};
use rustclamp_kernel::{ApplicationBlueprint, InclusionReason, ProcessProjection, ProjectionError};

const APP: ApplicationId = ApplicationId::new("test.application");
const CLI: ProcessId = ProcessId::new("test.process.cli");
const WORKER: ProcessId = ProcessId::new("test.process.worker");
const COMBINED: ProcessId = ProcessId::new("test.process.combined");
const CLI_EXEC: ExecutionId = ExecutionId::new("test.execution.cli");
const WORKER_EXEC: ExecutionId = ExecutionId::new("test.execution.worker");
const CLI_ROOT: ModuleId = ModuleId::new("test.module.cli-root");
const WORKER_ROOT: ModuleId = ModuleId::new("test.module.worker-root");
const HELLO: ModuleId = ModuleId::new("test.module.hello");
const CLOCK: ModuleId = ModuleId::new("test.module.clock");
const EPOCH: ModuleId = ModuleId::new("test.module.epoch");
const QUEUE: ModuleId = ModuleId::new("test.module.queue");
const DORMANT: ModuleId = ModuleId::new("test.module.dormant");
const CLOCK_CAP: CapabilityId = CapabilityId::new("test.capability.clock");
const EPOCH_CAP: CapabilityId = CapabilityId::new("test.capability.epoch");
const QUEUE_CAP: CapabilityId = CapabilityId::new("test.capability.queue");
const CLI_TARGET: ContributionTargetId = ContributionTargetId::new("test.target.cli");
const PUBLIC: QualifierId = QualifierId::new("test.qualifier.public");
const HELLO_CONTRIBUTION: ContributionId = ContributionId::new("test.contribution.hello");

fn blueprint() -> ApplicationBlueprint {
    let mut app = ApplicationBlueprint::new(APP);
    for module in [CLI_ROOT, WORKER_ROOT, HELLO, CLOCK, EPOCH, QUEUE, DORMANT] {
        app.add_module(module);
    }
    app.add_execution(CLI_EXEC, CLI_ROOT)
        .add_execution(WORKER_EXEC, WORKER_ROOT)
        .add_process(CLI, vec![CLI_EXEC])
        .add_process(WORKER, vec![WORKER_EXEC])
        .add_process(COMBINED, vec![WORKER_EXEC, CLI_EXEC])
        .consume_target(CLI_ROOT, CLI_TARGET, PUBLIC)
        .add_contribution(HELLO, CLI_TARGET, PUBLIC, HELLO_CONTRIBUTION)
        .require_provider(HELLO, CLOCK_CAP, None, CLOCK)
        .require_provider(CLOCK, EPOCH_CAP, None, EPOCH)
        .require_provider(WORKER_ROOT, CLOCK_CAP, None, CLOCK)
        .require_provider(WORKER_ROOT, QUEUE_CAP, None, QUEUE);
    app
}

fn projection_has(projection: &ProcessProjection, module: ModuleId) -> bool {
    projection.includes(module)
}

#[test]
fn cli_and_worker_are_distinct_projections_of_one_application() {
    let app = blueprint();
    let cli = app.project(CLI).unwrap();
    let worker = app.project(WORKER).unwrap();

    assert_eq!(cli.application(), APP);
    assert_eq!(cli.process(), CLI);
    assert_eq!(cli.roots(), [CLI_EXEC]);
    assert!(projection_has(&cli, CLI_ROOT));
    assert!(projection_has(&cli, HELLO));
    assert!(projection_has(&cli, CLOCK));
    assert!(projection_has(&cli, EPOCH));
    assert!(!projection_has(&cli, WORKER_ROOT));
    assert!(!projection_has(&cli, QUEUE));
    assert!(cli.excluded_modules().contains(&DORMANT));

    assert!(projection_has(&worker, WORKER_ROOT));
    assert!(projection_has(&worker, CLOCK));
    assert!(projection_has(&worker, EPOCH));
    assert!(projection_has(&worker, QUEUE));
    assert!(!projection_has(&worker, CLI_ROOT));
    assert!(!projection_has(&worker, HELLO));
}

#[test]
fn projection_tracks_contributor_and_transitive_provider_paths() {
    let projection = blueprint().project(CLI).unwrap();
    let hello = projection
        .included_modules()
        .iter()
        .find(|entry| entry.module() == HELLO)
        .unwrap();
    let epoch = projection
        .included_modules()
        .iter()
        .find(|entry| entry.module() == EPOCH)
        .unwrap();

    assert_eq!(hello.path(), [CLI_ROOT, HELLO]);
    assert_eq!(
        hello.reason(),
        InclusionReason::Contribution {
            consumed_by: CLI_ROOT,
            target: CLI_TARGET,
            qualifier: PUBLIC,
            contribution: HELLO_CONTRIBUTION,
        }
    );
    assert_eq!(epoch.path(), [CLI_ROOT, HELLO, CLOCK, EPOCH]);
    assert_eq!(
        epoch.reason(),
        InclusionReason::CapabilityProvider {
            required_by: CLOCK,
            capability: EPOCH_CAP,
            qualifier: None,
            selection: rustclamp_kernel::ProviderSelection::Explicit,
            replaced: None,
        }
    );
}

#[test]
fn projection_records_selection_exclusion_and_inspection_relations() {
    let explicit_exclusion = ModuleId::new("test.module.explicitly-excluded");
    let mut app = blueprint();
    app.add_module(explicit_exclusion)
        .exclude_module(explicit_exclusion);
    let projection = app.project(CLI).unwrap();
    let requirement = projection
        .requirements()
        .iter()
        .find(|entry| entry.consumer() == HELLO)
        .unwrap();
    assert_eq!(requirement.provider(), Some(CLOCK));
    assert_eq!(
        requirement.selection(),
        rustclamp_kernel::ProviderSelection::Explicit
    );
    let contribution = projection.contributions().first().unwrap();
    assert_eq!(contribution.consumer(), CLI_ROOT);
    assert_eq!(contribution.contributor(), HELLO);
    assert!(projection.exclusions().iter().any(|entry| {
        entry.module() == DORMANT
            && entry.reason() == rustclamp_kernel::ExclusionReason::Unreachable
    }));
    assert!(projection.exclusions().iter().any(|entry| {
        entry.module() == explicit_exclusion
            && entry.reason() == rustclamp_kernel::ExclusionReason::Explicit
    }));
}

#[test]
fn freezing_consumes_the_blueprint_and_runtime_matches_inspection() {
    let frozen = blueprint().freeze(CLI).unwrap();
    let runtime_modules = frozen.runtime().modules();
    let inspected_modules = frozen
        .inspection()
        .included_modules()
        .iter()
        .map(|entry| entry.module())
        .collect::<Vec<_>>();
    assert_eq!(runtime_modules, inspected_modules);
}

#[test]
fn one_frozen_application_produces_distinct_cli_and_worker_runtime_plans() {
    let frozen = blueprint().freeze_processes(&[CLI, WORKER]).unwrap();
    assert_eq!(frozen[0].inspection().process(), CLI);
    assert_eq!(frozen[1].inspection().process(), WORKER);
    assert!(frozen[0].runtime().modules().contains(&HELLO));
    assert!(!frozen[0].runtime().modules().contains(&QUEUE));
    assert!(frozen[1].runtime().modules().contains(&QUEUE));
    assert!(!frozen[1].runtime().modules().contains(&HELLO));
}

#[test]
fn shared_provider_is_included_once_for_a_process_with_two_roots() {
    let combined = blueprint().project(COMBINED).unwrap();
    assert_eq!(combined.roots(), [CLI_EXEC, WORKER_EXEC]);
    assert_eq!(
        combined
            .included_modules()
            .iter()
            .filter(|entry| entry.module() == CLOCK)
            .count(),
        1
    );
}

#[test]
fn projection_reports_missing_process_roots_and_reachable_modules() {
    let mut app = blueprint();
    let unknown_process = ProcessId::new("test.process.unknown");
    assert_eq!(
        app.project(unknown_process),
        Err(ProjectionError::MissingProcess {
            application: APP,
            process: unknown_process,
        })
    );

    let missing_execution = ExecutionId::new("test.execution.missing");
    let broken_process = ProcessId::new("test.process.broken");
    app.add_process(broken_process, vec![missing_execution]);
    assert_eq!(
        app.project(broken_process),
        Err(ProjectionError::MissingExecution {
            application: APP,
            process: broken_process,
            execution: missing_execution,
        })
    );

    let missing_module = ModuleId::new("test.module.missing");
    let broken_root = ExecutionId::new("test.execution.broken");
    app.add_execution(broken_root, missing_module)
        .add_process(broken_process, vec![broken_root]);
    assert_eq!(
        app.project(broken_process),
        Err(ProjectionError::MissingModule {
            application: APP,
            process: broken_process,
            module: missing_module,
        })
    );

    let rootless_process = ProcessId::new("test.process.rootless");
    app.add_process(rootless_process, Vec::new());
    assert_eq!(
        app.project(rootless_process),
        Err(ProjectionError::EmptyProcessRoots {
            application: APP,
            process: rootless_process,
        })
    );
}

#[test]
fn provider_resolution_reports_missing_and_ambiguous_requirements() {
    let consumer = ModuleId::new("test.module.consumer");
    let first = ModuleId::new("test.module.first");
    let second = ModuleId::new("test.module.second");
    let capability = CapabilityId::new("test.capability.resolve");
    let execution = ExecutionId::new("test.execution.resolve");
    let process = ProcessId::new("test.process.resolve");
    let mut app = ApplicationBlueprint::new(APP);
    app.add_module(consumer)
        .add_module(first)
        .add_module(second)
        .add_execution(execution, consumer)
        .add_process(process, vec![execution])
        .require_capability(consumer, capability, None, false);
    assert!(matches!(
        app.project(process),
        Err(ProjectionError::MissingProvider { .. })
    ));
    app.provide_capability(first, capability, None)
        .provide_capability(second, capability, None);
    assert!(matches!(
        app.project(process),
        Err(ProjectionError::AmbiguousProvider { .. })
    ));
    app.select_provider(consumer, capability, None, second);
    assert!(app.project(process).unwrap().includes(second));
}

#[test]
fn explicit_provider_overrides_defaults_while_conflicting_defaults_fail() {
    let consumer = ModuleId::new("test.module.default-consumer");
    let first = ModuleId::new("test.module.default-first");
    let second = ModuleId::new("test.module.default-second");
    let capability = CapabilityId::new("test.capability.default");
    let execution = ExecutionId::new("test.execution.default");
    let process = ProcessId::new("test.process.default");
    let mut app = ApplicationBlueprint::new(APP);
    app.add_module(consumer)
        .add_module(first)
        .add_module(second)
        .add_execution(execution, consumer)
        .add_process(process, vec![execution])
        .require_capability(consumer, capability, None, false)
        .provide_capability(first, capability, None)
        .provide_capability(second, capability, None)
        .default_provider(consumer, capability, None, first)
        .default_provider(consumer, capability, None, second);

    assert!(matches!(
        app.project(process),
        Err(ProjectionError::AmbiguousProvider { candidates, .. })
            if candidates == vec![first, second]
    ));
    app.select_provider(consumer, capability, None, second);
    let projection = app.project(process).unwrap();
    assert!(projection.includes(second));
    assert!(!projection.includes(first));

    let mut default_app = ApplicationBlueprint::new(APP);
    default_app
        .add_module(consumer)
        .add_module(first)
        .add_execution(execution, consumer)
        .add_process(process, vec![execution])
        .require_capability(consumer, capability, None, false)
        .provide_capability(first, capability, None)
        .default_provider(consumer, capability, None, first);
    assert!(matches!(
        default_app
            .project(process)
            .unwrap()
            .included_modules()
            .iter()
            .find(|entry| entry.module() == first)
            .unwrap()
            .reason(),
        InclusionReason::CapabilityProvider {
            selection: rustclamp_kernel::ProviderSelection::Default,
            replaced: None,
            ..
        }
    ));
}

#[test]
fn replacement_recomputes_reachable_provider_subtree_and_exclusions_apply() {
    let consumer = ModuleId::new("test.module.replace-consumer");
    let old = ModuleId::new("test.module.old-provider");
    let replacement = ModuleId::new("test.module.new-provider");
    let old_only = ModuleId::new("test.module.old-only");
    let new_only = ModuleId::new("test.module.new-only");
    let capability = CapabilityId::new("test.capability.replace");
    let downstream = CapabilityId::new("test.capability.downstream");
    let execution = ExecutionId::new("test.execution.replace");
    let process = ProcessId::new("test.process.replace");
    let mut app = ApplicationBlueprint::new(APP);
    for module in [consumer, old, replacement, old_only, new_only] {
        app.add_module(module);
    }
    app.add_execution(execution, consumer)
        .add_process(process, vec![execution])
        .require_capability(consumer, capability, None, false)
        .provide_capability(old, capability, None)
        .provide_capability(replacement, capability, None)
        .replace_provider(capability, None, old, replacement)
        .require_provider(old, downstream, None, old_only)
        .require_provider(replacement, downstream, None, new_only);
    let projection = app.project(process).unwrap();
    assert!(projection.includes(replacement));
    assert!(projection.includes(new_only));
    assert!(!projection.includes(old));
    assert!(!projection.includes(old_only));
    assert!(matches!(
        projection
            .included_modules()
            .iter()
            .find(|entry| entry.module() == replacement)
            .unwrap()
            .reason(),
        InclusionReason::CapabilityProvider {
            selection: rustclamp_kernel::ProviderSelection::Unique,
            replaced: Some(module),
            ..
        } if module == old
    ));

    app.exclude_module(replacement);
    assert!(matches!(
        app.project(process),
        Err(ProjectionError::ExcludedProvider { .. })
    ));
}

#[test]
fn replacement_must_declare_the_capability_it_replaces() {
    let consumer = ModuleId::new("test.module.broken-replacement-consumer");
    let original = ModuleId::new("test.module.broken-replacement-original");
    let replacement = ModuleId::new("test.module.broken-replacement-target");
    let capability = CapabilityId::new("test.capability.broken-replacement");
    let execution = ExecutionId::new("test.execution.broken-replacement");
    let process = ProcessId::new("test.process.broken-replacement");
    let mut app = ApplicationBlueprint::new(APP);
    app.add_module(consumer)
        .add_module(original)
        .add_module(replacement)
        .add_execution(execution, consumer)
        .add_process(process, vec![execution])
        .require_capability(consumer, capability, None, false)
        .provide_capability(original, capability, None)
        .replace_provider(capability, None, original, replacement);
    assert!(matches!(
        app.project(process),
        Err(ProjectionError::UnavailableProvider { provider, .. }) if provider == replacement
    ));
}

#[test]
fn optional_requirements_do_not_activate_providers() {
    let consumer = ModuleId::new("test.module.optional-consumer");
    let provider = ModuleId::new("test.module.optional-provider");
    let capability = CapabilityId::new("test.capability.optional");
    let execution = ExecutionId::new("test.execution.optional");
    let process = ProcessId::new("test.process.optional");
    let mut app = ApplicationBlueprint::new(APP);
    app.add_module(consumer)
        .add_module(provider)
        .add_execution(execution, consumer)
        .add_process(process, vec![execution])
        .provide_capability(provider, capability, None)
        .require_capability(consumer, capability, None, true);
    let projection = app.project(process).unwrap();
    assert!(!projection.includes(provider));
}

#[test]
fn reachable_dependency_cycles_report_a_closed_path() {
    let first = ModuleId::new("test.module.cycle-first");
    let second = ModuleId::new("test.module.cycle-second");
    let first_capability = CapabilityId::new("test.capability.cycle-first");
    let second_capability = CapabilityId::new("test.capability.cycle-second");
    let execution = ExecutionId::new("test.execution.cycle");
    let process = ProcessId::new("test.process.cycle");
    let mut app = ApplicationBlueprint::new(APP);
    app.add_module(first)
        .add_module(second)
        .add_execution(execution, first)
        .add_process(process, vec![execution])
        .require_provider(first, first_capability, None, second)
        .require_provider(second, second_capability, None, first);

    let Err(ProjectionError::DependencyCycle { path, .. }) = app.project(process) else {
        panic!("expected a dependency cycle");
    };
    assert_eq!(path.first(), path.last());
    assert_eq!(path.len(), 3);
}

#[test]
fn required_orphan_contribution_errors_only_when_its_module_is_reachable() {
    let consumer = ModuleId::new("test.module.contribution-consumer");
    let contributor = ModuleId::new("test.module.orphan-contributor");
    let dormant = ModuleId::new("test.module.dormant-orphan");
    let capability = CapabilityId::new("test.capability.contribution");
    let target = ContributionTargetId::new("test.target.unconsumed");
    let contribution = ContributionId::new("test.contribution.orphan");
    let execution = ExecutionId::new("test.execution.contribution");
    let process = ProcessId::new("test.process.contribution");
    let mut app = ApplicationBlueprint::new(APP);
    app.add_module(consumer)
        .add_module(contributor)
        .add_module(dormant)
        .add_execution(execution, consumer)
        .add_process(process, vec![execution])
        .require_provider(consumer, capability, None, contributor)
        .add_contribution(contributor, target, PUBLIC, contribution)
        .add_contribution(dormant, target, PUBLIC, contribution);

    assert!(matches!(
        app.project(process),
        Err(ProjectionError::OrphanContribution {
            contributor: actual,
            ..
        }) if actual == contributor
    ));

    let mut dormant_only = ApplicationBlueprint::new(APP);
    dormant_only
        .add_module(consumer)
        .add_module(dormant)
        .add_execution(execution, consumer)
        .add_process(process, vec![execution])
        .add_contribution(dormant, target, PUBLIC, contribution);
    assert!(dormant_only.project(process).is_ok());
}

#[test]
fn qualified_contributions_stay_with_their_process_and_diagnostics_name_process() {
    let public_root = ModuleId::new("test.module.public-root");
    let private_root = ModuleId::new("test.module.private-root");
    let public_contributor = ModuleId::new("test.module.public-contributor");
    let private_contributor = ModuleId::new("test.module.private-contributor");
    let target = ContributionTargetId::new("test.target.qualified");
    let public_qualifier = QualifierId::new("test.qualifier.target-public");
    let private_qualifier = QualifierId::new("test.qualifier.target-private");
    let public_execution = ExecutionId::new("test.execution.public");
    let private_execution = ExecutionId::new("test.execution.private");
    let public_process = ProcessId::new("test.process.public");
    let private_process = ProcessId::new("test.process.private");
    let missing_capability = CapabilityId::new("test.capability.missing-for-private");
    let mut app = ApplicationBlueprint::new(APP);
    for module in [
        public_root,
        private_root,
        public_contributor,
        private_contributor,
    ] {
        app.add_module(module);
    }
    app.add_execution(public_execution, public_root)
        .add_execution(private_execution, private_root)
        .add_process(public_process, vec![public_execution])
        .add_process(private_process, vec![private_execution])
        .consume_target(public_root, target, public_qualifier)
        .consume_target(private_root, target, private_qualifier)
        .add_contribution(
            public_contributor,
            target,
            public_qualifier,
            HELLO_CONTRIBUTION,
        )
        .add_contribution(
            private_contributor,
            target,
            private_qualifier,
            HELLO_CONTRIBUTION,
        );

    let public = app.project(public_process).unwrap();
    assert!(public.includes(public_contributor));
    assert!(!public.includes(private_contributor));
    let private = app.project(private_process).unwrap();
    assert!(private.includes(private_contributor));
    assert!(!private.includes(public_contributor));

    app.require_capability(private_root, missing_capability, None, false);
    assert!(matches!(
        app.project(private_process),
        Err(ProjectionError::MissingProvider {
            application: actual_application,
            process: actual_process,
            ..
        }) if actual_application == APP && actual_process == private_process
    ));
    assert!(app.project(public_process).is_ok());
}
