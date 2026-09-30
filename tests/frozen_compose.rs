//! Tests for building contribution targets through a frozen process.

use std::convert::Infallible;

use rustclamp_core::{
    ApplicationId, Contribution, ContributionId, ContributionTarget, ContributionTargetId,
    ExecutionId, ModuleId, ProcessId, Qualifier, QualifierId,
};
use rustclamp_kernel::{ApplicationBlueprint, ComposeError, FrozenProcess};

const APP: ApplicationId = ApplicationId::new("test.application");
const WORKER: ProcessId = ProcessId::new("test.process.worker");
const CLI: ProcessId = ProcessId::new("test.process.cli");
const WORKER_EXEC: ExecutionId = ExecutionId::new("test.execution.worker");
const CLI_EXEC: ExecutionId = ExecutionId::new("test.execution.cli");
const WORKER_ROOT: ModuleId = ModuleId::new("test.module.worker-root");
const CLI_ROOT: ModuleId = ModuleId::new("test.module.cli-root");
const ECHO: ModuleId = ModuleId::new("test.module.echo");
const SLEEP: ModuleId = ModuleId::new("test.module.sleep");

struct Route(&'static str);

impl Contribution for Route {
    const ID: ContributionId = ContributionId::new("test.contribution.route");
}

struct Routes;

impl ContributionTarget for Routes {
    type Contribution = Route;
    type Runtime = Vec<&'static str>;
    type Error = Infallible;
    const ID: ContributionTargetId = ContributionTargetId::new("test.target.routes");

    fn build(&self, contributions: &[(ModuleId, Route)]) -> Result<Vec<&'static str>, Infallible> {
        Ok(contributions.iter().map(|(_, route)| route.0).collect())
    }
}

struct Handlers;

impl Qualifier for Handlers {
    const ID: QualifierId = QualifierId::new("test.qualifier.handlers");
}

/// The worker consumes routes from ECHO and SLEEP; the CLI consumes only ECHO's.
fn freeze(process: ProcessId) -> FrozenProcess {
    let mut app = ApplicationBlueprint::new(APP);
    app.add_root(WORKER, WORKER_EXEC, WORKER_ROOT)
        .add_root(CLI, CLI_EXEC, CLI_ROOT)
        .add_module(ECHO)
        .add_module(SLEEP)
        .consume_target(WORKER_ROOT, Routes::ID, Handlers::ID)
        .add_contribution(ECHO, Routes::ID, Handlers::ID, Route::ID)
        .add_contribution(SLEEP, Routes::ID, Handlers::ID, Route::ID);
    app.freeze(process).unwrap()
}

fn supplied() -> Vec<(ModuleId, Route)> {
    vec![(ECHO, Route("echo")), (SLEEP, Route("sleep"))]
}

#[test]
fn compose_builds_from_the_frozen_contributors() {
    let routes = freeze(WORKER)
        .compose::<Routes, Handlers>(&Routes, supplied())
        .unwrap();
    assert_eq!(routes, ["echo", "sleep"]);
}

#[test]
fn contributions_from_modules_outside_the_process_are_dropped() {
    let routes = freeze(CLI)
        .compose::<Routes, Handlers>(&Routes, supplied())
        .unwrap();
    assert!(routes.is_empty());
}

#[test]
fn a_declared_contributor_that_supplies_nothing_fails() {
    let error = freeze(WORKER)
        .compose::<Routes, Handlers>(&Routes, vec![(ECHO, Route("echo"))])
        .unwrap_err();
    assert_eq!(
        error,
        ComposeError::MissingContribution {
            target: Routes::ID,
            qualifier: Handlers::ID,
            contributor: SLEEP,
        }
    );
}

#[test]
fn an_included_module_without_an_edge_fails() {
    let mut contributions = supplied();
    contributions.push((WORKER_ROOT, Route("stray")));
    let error = freeze(WORKER)
        .compose::<Routes, Handlers>(&Routes, contributions)
        .unwrap_err();
    assert!(matches!(
        error,
        ComposeError::UndeclaredContribution {
            contributor: WORKER_ROOT,
            ..
        }
    ));
    assert!(error.to_string().contains("without a blueprint edge"));
}

#[test]
fn compose_inferred_reads_the_qualifier_from_the_blueprint() {
    let worker = freeze(WORKER);
    assert_eq!(
        worker.compose_inferred(&Routes, supplied()),
        worker.compose::<Routes, Handlers>(&Routes, supplied())
    );
    assert_eq!(
        worker.compose_inferred(&Routes, supplied()).unwrap(),
        ["echo", "sleep"]
    );
}

#[test]
fn compose_inferred_refuses_to_guess_between_qualifiers() {
    struct Admin;
    impl Qualifier for Admin {
        const ID: QualifierId = QualifierId::new("test.qualifier.admin");
    }
    let mut app = ApplicationBlueprint::new(APP);
    app.add_root(WORKER, WORKER_EXEC, WORKER_ROOT)
        .add_module(ECHO)
        .add_module(SLEEP)
        .consume_target(WORKER_ROOT, Routes::ID, Handlers::ID)
        .consume_target(WORKER_ROOT, Routes::ID, Admin::ID)
        .add_contribution(ECHO, Routes::ID, Handlers::ID, Route::ID)
        .add_contribution(SLEEP, Routes::ID, Admin::ID, Route::ID);
    let worker = app.freeze(WORKER).unwrap();
    let error = worker.compose_inferred(&Routes, supplied()).unwrap_err();
    assert_eq!(
        error,
        ComposeError::AmbiguousQualifier {
            target: Routes::ID,
            qualifiers: vec![Admin::ID, Handlers::ID],
        }
    );
    assert!(error.to_string().contains("compose::<T, Q>"), "{error}");
}
