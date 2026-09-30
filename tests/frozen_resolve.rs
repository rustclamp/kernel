//! Tests for resolving capability values through a frozen process.

use rustclamp_core::{ApplicationId, Capability, CapabilityId, ExecutionId, ModuleId, ProcessId};
use rustclamp_kernel::{
    ApplicationBlueprint, CompositionErrorKind, FrozenProcess, ProjectionError, Provision,
};

const APP: ApplicationId = ApplicationId::new("test.application");
const CLI: ProcessId = ProcessId::new("test.process.cli");
const CLI_EXEC: ExecutionId = ExecutionId::new("test.execution.cli");
const ROOT: ModuleId = ModuleId::new("test.module.root");
const PRIMARY: ModuleId = ModuleId::new("test.module.primary");
const BACKUP: ModuleId = ModuleId::new("test.module.backup");

struct Greeting;

impl Capability for Greeting {
    type Value = str;
    const ID: CapabilityId = CapabilityId::new("test.capability.greeting");
}

fn frozen() -> FrozenProcess {
    let mut app = ApplicationBlueprint::new(APP);
    app.add_root(CLI, CLI_EXEC, ROOT)
        .add_module(PRIMARY)
        .add_module(BACKUP)
        .provide_capability(BACKUP, Greeting::ID, None)
        .require_provider(ROOT, Greeting::ID, None, PRIMARY);
    app.freeze(CLI).unwrap()
}

#[test]
fn resolve_uses_the_provider_selected_at_freeze() {
    let provisions = [
        Provision::<Greeting>::new(BACKUP, "backup"),
        Provision::<Greeting>::new(PRIMARY, "primary"),
    ];
    assert_eq!(frozen().resolve(ROOT, &provisions).unwrap(), "primary");
}

#[test]
fn resolve_rejects_a_requirement_the_process_never_declared() {
    let provisions = [Provision::<Greeting>::new(PRIMARY, "primary")];
    let error = frozen().resolve(PRIMARY, &provisions).unwrap_err();
    assert_eq!(error.kind(), CompositionErrorKind::UndeclaredRequirement);
    assert_eq!(error.candidates(), [PRIMARY]);
}

#[test]
fn resolve_reports_a_selected_provider_without_a_value() {
    let provisions = [Provision::<Greeting>::new(BACKUP, "backup")];
    let error = frozen().resolve(ROOT, &provisions).unwrap_err();
    assert_eq!(
        error.kind(),
        CompositionErrorKind::ProviderSelectionUnavailable
    );
}

#[test]
fn add_root_declares_module_execution_and_process() {
    let frozen = frozen();
    assert_eq!(frozen.inspection().roots(), [CLI_EXEC]);
    assert!(frozen.inspection().includes(ROOT));
}

#[test]
fn projection_errors_render_for_humans() {
    let error = ApplicationBlueprint::new(APP).freeze(CLI).unwrap_err();
    assert!(matches!(error, ProjectionError::MissingProcess { .. }));
    assert_eq!(
        error.to_string(),
        "process 'test.application/test.process.cli': process is not declared"
    );

    let mut app = ApplicationBlueprint::new(APP);
    app.add_root(CLI, CLI_EXEC, ROOT)
        .require_capability(ROOT, Greeting::ID, None, false);
    let error: Box<dyn std::error::Error> = Box::new(app.freeze(CLI).unwrap_err());
    assert_eq!(
        error.to_string(),
        "process 'test.application/test.process.cli': no provider for capability \
         'test.capability.greeting' required by module 'test.module.root'"
    );
}
