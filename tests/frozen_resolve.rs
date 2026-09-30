//! Tests for resolving capability values through a frozen process.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use rustclamp_core::{ApplicationId, Capability, CapabilityId, ExecutionId, ModuleId, ProcessId};
use rustclamp_kernel::{
    ApplicationBlueprint, CompositionErrorKind, FactoryError, FactoryProvision, FrozenProcess,
    ProjectionError, Provision, SharedProvision,
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

#[test]
fn resolve_shared_returns_an_owned_handle_of_the_selected_provider() {
    let value: Arc<str> = Arc::from("primary");
    let provisions = [
        SharedProvision::<Greeting>::new(BACKUP, Arc::from("backup")),
        SharedProvision::<Greeting>::new(PRIMARY, Arc::clone(&value)),
    ];
    let resolved = frozen().resolve_shared(ROOT, &provisions).unwrap();
    drop(provisions);
    assert!(Arc::ptr_eq(&resolved, &value));
}

#[test]
fn resolve_with_runs_only_the_selected_factory() {
    let backup_ran = Rc::new(Cell::new(false));
    let flag = Rc::clone(&backup_ran);
    let provisions = vec![
        FactoryProvision::<Greeting, String>::new(BACKUP, move || {
            flag.set(true);
            Ok(Arc::from("backup"))
        }),
        FactoryProvision::new(PRIMARY, || Ok(Arc::from("primary"))),
    ];
    assert_eq!(
        &*frozen().resolve_with(ROOT, provisions).unwrap(),
        "primary"
    );
    assert!(!backup_ran.get());
}

#[test]
fn resolve_with_reports_a_failed_factory() {
    let provisions = vec![FactoryProvision::<Greeting, String>::new(PRIMARY, || {
        Err("open failed".to_string())
    })];
    assert_eq!(
        frozen().resolve_with(ROOT, provisions).unwrap_err(),
        FactoryError::Build("open failed".to_string())
    );
}

#[test]
fn resolve_with_reports_selection_errors_without_building() {
    let provisions = vec![FactoryProvision::<Greeting, String>::new(BACKUP, || {
        panic!("must not build")
    })];
    let FactoryError::Composition(error) = frozen().resolve_with(ROOT, provisions).unwrap_err()
    else {
        panic!("expected composition error");
    };
    assert_eq!(
        error.kind(),
        CompositionErrorKind::ProviderSelectionUnavailable
    );
}
