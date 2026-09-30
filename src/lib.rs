//! Typed capability resolution for Clamp applications.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::marker::PhantomData;

use rustclamp_core::{
    ApplicationId, Capability, CapabilityId, Contribution, ContributionId, ContributionTarget,
    ContributionTargetId, ExecutionId, Module, ModuleId, ProcessId, Provides, Qualifier,
    QualifierId, Requires,
};

/// A caller-owned application blueprint containing process roots and module relations.
///
/// The blueprint describes declarations only. [`ApplicationBlueprint::project`]
/// derives the reachable module set for one process without constructing modules.
pub struct ApplicationBlueprint {
    application: ApplicationId,
    modules: BTreeSet<ModuleId>,
    executions: BTreeMap<ExecutionId, ModuleId>,
    processes: BTreeMap<ProcessId, Vec<ExecutionId>>,
    providers: Vec<(ModuleId, CapabilityId, Option<QualifierId>)>,
    requirements: Vec<(ModuleId, CapabilityId, Option<QualifierId>, bool)>,
    selections: Vec<(ModuleId, CapabilityId, Option<QualifierId>, ModuleId)>,
    defaults: Vec<(ModuleId, CapabilityId, Option<QualifierId>, ModuleId)>,
    replacements: Vec<(CapabilityId, Option<QualifierId>, ModuleId, ModuleId)>,
    exclusions: BTreeSet<ModuleId>,
    target_consumptions: Vec<(ModuleId, ContributionTargetId, QualifierId)>,
    contributions: Vec<(
        ModuleId,
        ContributionTargetId,
        QualifierId,
        ContributionId,
        bool,
    )>,
}

impl ApplicationBlueprint {
    /// Creates an empty architecture blueprint with a stable identity.
    pub fn new(application: ApplicationId) -> Self {
        Self {
            application,
            modules: BTreeSet::new(),
            executions: BTreeMap::new(),
            processes: BTreeMap::new(),
            providers: Vec::new(),
            requirements: Vec::new(),
            selections: Vec::new(),
            defaults: Vec::new(),
            replacements: Vec::new(),
            exclusions: BTreeSet::new(),
            target_consumptions: Vec::new(),
            contributions: Vec::new(),
        }
    }

    /// Declares a module that may be included in one or more process projections.
    pub fn add_module(&mut self, module: ModuleId) -> &mut Self {
        self.modules.insert(module);
        self
    }

    /// Declares an execution root implemented by a module.
    pub fn add_execution(&mut self, execution: ExecutionId, module: ModuleId) -> &mut Self {
        self.executions.insert(execution, module);
        self
    }

    /// Declares one or more execution roots for a runnable process projection.
    pub fn add_process(&mut self, process: ProcessId, roots: Vec<ExecutionId>) -> &mut Self {
        self.processes.insert(process, roots);
        self
    }

    /// Declares a process with one execution root: the module, its execution, and
    /// the process that runs only that execution.
    pub fn add_root(
        &mut self,
        process: ProcessId,
        execution: ExecutionId,
        module: ModuleId,
    ) -> &mut Self {
        self.add_module(module)
            .add_execution(execution, module)
            .add_process(process, vec![execution])
    }

    /// Connects a module requirement to its already selected capability provider.
    pub fn require_provider(
        &mut self,
        consumer: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        provider: ModuleId,
    ) -> &mut Self {
        self.provide_capability(provider, capability, qualifier)
            .require_capability(consumer, capability, qualifier, false)
            .select_provider(consumer, capability, qualifier, provider);
        self
    }

    /// Declares that a module provides a capability for an optional qualifier.
    pub fn provide_capability(
        &mut self,
        module: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
    ) -> &mut Self {
        self.providers.push((module, capability, qualifier));
        self
    }

    /// Declares a capability requirement. Optional requirements do not activate a provider.
    pub fn require_capability(
        &mut self,
        consumer: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        optional: bool,
    ) -> &mut Self {
        self.requirements
            .push((consumer, capability, qualifier, optional));
        self
    }

    /// Selects a provider for one module requirement.
    pub fn select_provider(
        &mut self,
        consumer: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        provider: ModuleId,
    ) -> &mut Self {
        self.selections
            .push((consumer, capability, qualifier, provider));
        self
    }

    /// Declares a default provider for one module requirement.
    pub fn default_provider(
        &mut self,
        consumer: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        provider: ModuleId,
    ) -> &mut Self {
        self.defaults
            .push((consumer, capability, qualifier, provider));
        self
    }

    /// Replaces one provider with another for a capability and qualifier.
    pub fn replace_provider(
        &mut self,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        replaced: ModuleId,
        replacement: ModuleId,
    ) -> &mut Self {
        self.replacements
            .push((capability, qualifier, replaced, replacement));
        self
    }

    /// Excludes a module from provider selection and process reachability.
    pub fn exclude_module(&mut self, module: ModuleId) -> &mut Self {
        self.exclusions.insert(module);
        self
    }

    /// Declares that a module consumes a qualified contribution target.
    pub fn consume_target(
        &mut self,
        consumer: ModuleId,
        target: ContributionTargetId,
        qualifier: QualifierId,
    ) -> &mut Self {
        self.target_consumptions.push((consumer, target, qualifier));
        self
    }

    /// Declares that a module contributes to a qualified target.
    pub fn add_contribution(
        &mut self,
        contributor: ModuleId,
        target: ContributionTargetId,
        qualifier: QualifierId,
        contribution: ContributionId,
    ) -> &mut Self {
        self.contributions
            .push((contributor, target, qualifier, contribution, true));
        self
    }

    /// Declares an optional contribution that may be discarded without a target.
    pub fn add_optional_contribution(
        &mut self,
        contributor: ModuleId,
        target: ContributionTargetId,
        qualifier: QualifierId,
        contribution: ContributionId,
    ) -> &mut Self {
        self.contributions
            .push((contributor, target, qualifier, contribution, false));
        self
    }

    /// Derives the reachable module projection from a process's execution roots.
    pub fn project(&self, process: ProcessId) -> Result<ProcessProjection, ProjectionError> {
        let application = self.application;
        let mut providers_by_capability =
            BTreeMap::<(CapabilityId, Option<QualifierId>), Vec<ModuleId>>::new();
        for (module, capability, qualifier) in &self.providers {
            providers_by_capability
                .entry((*capability, *qualifier))
                .or_default()
                .push(*module);
        }
        for providers in providers_by_capability.values_mut() {
            providers.sort_unstable();
            providers.dedup();
        }
        let mut requirements_by_owner =
            BTreeMap::<ModuleId, Vec<(CapabilityId, Option<QualifierId>, bool)>>::new();
        for (consumer, capability, qualifier, optional) in &self.requirements {
            requirements_by_owner.entry(*consumer).or_default().push((
                *capability,
                *qualifier,
                *optional,
            ));
        }
        let mut selections = BTreeMap::new();
        for (consumer, capability, qualifier, provider) in &self.selections {
            selections
                .entry((*consumer, *capability, *qualifier))
                .or_insert(*provider);
        }
        let mut defaults =
            BTreeMap::<(ModuleId, CapabilityId, Option<QualifierId>), Vec<ModuleId>>::new();
        for (consumer, capability, qualifier, provider) in &self.defaults {
            defaults
                .entry((*consumer, *capability, *qualifier))
                .or_default()
                .push(*provider);
        }
        for providers in defaults.values_mut() {
            providers.sort_unstable();
            providers.dedup();
        }
        let mut targets_by_owner =
            BTreeMap::<ModuleId, Vec<(ContributionTargetId, QualifierId)>>::new();
        let mut consumers_by_target =
            BTreeMap::<(ContributionTargetId, QualifierId), Vec<ModuleId>>::new();
        for (consumer, target, qualifier) in &self.target_consumptions {
            targets_by_owner
                .entry(*consumer)
                .or_default()
                .push((*target, *qualifier));
            consumers_by_target
                .entry((*target, *qualifier))
                .or_default()
                .push(*consumer);
        }
        let mut contributions_by_target =
            BTreeMap::<(ContributionTargetId, QualifierId), Vec<(ModuleId, ContributionId)>>::new();
        for (contributor, target, qualifier, contribution, _) in &self.contributions {
            contributions_by_target
                .entry((*target, *qualifier))
                .or_default()
                .push((*contributor, *contribution));
        }
        let replacements = self
            .replacements
            .iter()
            .map(|(capability, qualifier, old, new)| ((*capability, *qualifier, *old), *new))
            .collect::<BTreeMap<_, _>>();
        let replacement_sources = self
            .replacements
            .iter()
            .map(|(capability, qualifier, old, new)| ((*capability, *qualifier, *new), *old))
            .collect::<BTreeMap<_, _>>();
        let roots = self
            .processes
            .get(&process)
            .ok_or(ProjectionError::MissingProcess {
                application,
                process,
            })?;
        if roots.is_empty() {
            return Err(ProjectionError::EmptyProcessRoots {
                application,
                process,
            });
        }
        let mut roots = roots.clone();
        roots.sort_unstable();

        let mut pending = VecDeque::new();
        let mut reached = BTreeMap::<ModuleId, IncludedModule>::new();
        let mut edges = Vec::<(ModuleId, ModuleId)>::new();
        let mut resolved_requirements = Vec::new();
        let mut resolved_contributions = Vec::new();
        for execution in &roots {
            let module = self.executions.get(execution).copied().ok_or(
                ProjectionError::MissingExecution {
                    application,
                    process,
                    execution: *execution,
                },
            )?;
            self.ensure_module(application, process, module)?;
            if self.exclusions.contains(&module) {
                return Err(ProjectionError::ExcludedRoot {
                    application,
                    process,
                    module,
                });
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = reached.entry(module) {
                entry.insert(IncludedModule {
                    module,
                    path: vec![module],
                    reason: InclusionReason::ExecutionRoot {
                        execution: *execution,
                    },
                });
                pending.push_back(module);
            }
        }

        while let Some(consumer) = pending.pop_front() {
            let mut next = Vec::<(ModuleId, InclusionReason)>::new();
            for (capability, qualifier, optional) in
                requirements_by_owner.get(&consumer).into_iter().flatten()
            {
                let requirement_key = (consumer, *capability, *qualifier);
                let explicit = selections.get(&requirement_key).copied();
                let defaults = defaults.get(&requirement_key).cloned().unwrap_or_default();
                if explicit.is_none() && defaults.len() > 1 {
                    return Err(ProjectionError::AmbiguousProvider {
                        application,
                        process,
                        required_by: consumer,
                        capability: *capability,
                        qualifier: *qualifier,
                        candidates: defaults,
                    });
                }
                let selection = if explicit.is_some() {
                    ProviderSelection::Explicit
                } else if !defaults.is_empty() {
                    ProviderSelection::Default
                } else {
                    ProviderSelection::Unique
                };
                let selected = explicit.or_else(|| defaults.first().copied());
                let mut candidates = providers_by_capability
                    .get(&(*capability, *qualifier))
                    .cloned()
                    .unwrap_or_default();
                if let Some(chosen) = selected {
                    let provider = replacements
                        .get(&(*capability, *qualifier, chosen))
                        .copied()
                        .unwrap_or(chosen);
                    if self.exclusions.contains(&provider) {
                        return Err(ProjectionError::ExcludedProvider {
                            application,
                            process,
                            required_by: consumer,
                            capability: *capability,
                            qualifier: *qualifier,
                            provider,
                        });
                    }
                    if (!candidates.contains(&chosen) && !candidates.contains(&provider))
                        || !providers_by_capability
                            .get(&(*capability, *qualifier))
                            .is_some_and(|providers| providers.contains(&provider))
                    {
                        return Err(ProjectionError::UnavailableProvider {
                            application,
                            process,
                            required_by: consumer,
                            capability: *capability,
                            qualifier: *qualifier,
                            provider,
                        });
                    }
                    next.push((
                        provider,
                        InclusionReason::CapabilityProvider {
                            required_by: consumer,
                            capability: *capability,
                            qualifier: *qualifier,
                            selection,
                            replaced: (provider != chosen).then_some(chosen),
                        },
                    ));
                    resolved_requirements.push(ResolvedRequirement {
                        consumer,
                        capability: *capability,
                        qualifier: *qualifier,
                        provider: Some(provider),
                        optional: *optional,
                        selection,
                        replaced: (provider != chosen).then_some(chosen),
                    });
                    continue;
                }
                if *optional {
                    resolved_requirements.push(ResolvedRequirement {
                        consumer,
                        capability: *capability,
                        qualifier: *qualifier,
                        provider: None,
                        optional: true,
                        selection: ProviderSelection::OptionalAbsent,
                        replaced: None,
                    });
                    continue;
                }
                for candidate in &mut candidates {
                    *candidate = replacements
                        .get(&(*capability, *qualifier, *candidate))
                        .copied()
                        .unwrap_or(*candidate);
                }
                if let Some(provider) = candidates.iter().find(|candidate| {
                    !providers_by_capability
                        .get(&(*capability, *qualifier))
                        .is_some_and(|providers| providers.contains(candidate))
                }) {
                    return Err(ProjectionError::UnavailableProvider {
                        application,
                        process,
                        required_by: consumer,
                        capability: *capability,
                        qualifier: *qualifier,
                        provider: *provider,
                    });
                }
                candidates.retain(|module| {
                    providers_by_capability
                        .get(&(*capability, *qualifier))
                        .is_some_and(|providers| providers.contains(module))
                });
                candidates.retain(|module| !self.exclusions.contains(module));
                candidates.sort_unstable();
                candidates.dedup();
                match candidates.as_slice() {
                    [] if providers_by_capability.contains_key(&(*capability, *qualifier)) => {
                        let provider = providers_by_capability[&(*capability, *qualifier)][0];
                        return Err(ProjectionError::ExcludedProvider {
                            application,
                            process,
                            required_by: consumer,
                            capability: *capability,
                            qualifier: *qualifier,
                            provider,
                        });
                    }
                    [] => {
                        return Err(ProjectionError::MissingProvider {
                            application,
                            process,
                            required_by: consumer,
                            capability: *capability,
                            qualifier: *qualifier,
                        });
                    }
                    [provider] => {
                        let replaced = replacement_sources
                            .get(&(*capability, *qualifier, *provider))
                            .copied();
                        next.push((
                            *provider,
                            InclusionReason::CapabilityProvider {
                                required_by: consumer,
                                capability: *capability,
                                qualifier: *qualifier,
                                selection: ProviderSelection::Unique,
                                replaced,
                            },
                        ));
                        resolved_requirements.push(ResolvedRequirement {
                            consumer,
                            capability: *capability,
                            qualifier: *qualifier,
                            provider: Some(*provider),
                            optional: false,
                            selection: ProviderSelection::Unique,
                            replaced,
                        });
                    }
                    _ => {
                        return Err(ProjectionError::AmbiguousProvider {
                            application,
                            process,
                            required_by: consumer,
                            capability: *capability,
                            qualifier: *qualifier,
                            candidates,
                        });
                    }
                }
            }
            for (target, qualifier) in targets_by_owner.get(&consumer).into_iter().flatten() {
                for (contributor, contribution) in contributions_by_target
                    .get(&(*target, *qualifier))
                    .into_iter()
                    .flatten()
                {
                    resolved_contributions.push(ResolvedContribution {
                        consumer,
                        contributor: *contributor,
                        target: *target,
                        qualifier: *qualifier,
                        contribution: *contribution,
                    });
                    next.push((
                        *contributor,
                        InclusionReason::Contribution {
                            consumed_by: consumer,
                            target: *target,
                            qualifier: *qualifier,
                            contribution: *contribution,
                        },
                    ));
                }
            }
            next.sort_unstable();

            for (module, reason) in next {
                if self.exclusions.contains(&module) {
                    continue;
                }
                self.ensure_module(application, process, module)?;
                edges.push((consumer, module));
                if reached.contains_key(&module) {
                    continue;
                }
                let mut path = reached[&consumer].path.clone();
                path.push(module);
                reached.insert(
                    module,
                    IncludedModule {
                        module,
                        path,
                        reason,
                    },
                );
                pending.push_back(module);
            }
        }

        if let Some(path) = find_cycle(&edges) {
            return Err(ProjectionError::DependencyCycle {
                application,
                process,
                path,
            });
        }

        let included_set = reached.keys().copied().collect::<BTreeSet<_>>();
        for (contributor, target, qualifier, contribution, required) in &self.contributions {
            if *required && included_set.contains(contributor) {
                let consumed =
                    consumers_by_target
                        .get(&(*target, *qualifier))
                        .is_some_and(|consumers| {
                            consumers
                                .iter()
                                .any(|consumer| included_set.contains(consumer))
                        });
                if !consumed {
                    return Err(ProjectionError::OrphanContribution {
                        application,
                        process,
                        contributor: *contributor,
                        target: *target,
                        qualifier: *qualifier,
                        contribution: *contribution,
                    });
                }
            }
        }

        let included_modules = reached.into_values().collect::<Vec<_>>();
        let included_set = included_modules
            .iter()
            .map(|entry| entry.module)
            .collect::<BTreeSet<_>>();
        let excluded_modules = self.modules.difference(&included_set).copied().collect();
        let exclusions = self
            .modules
            .difference(&included_set)
            .map(|module| ExcludedModule {
                module: *module,
                reason: if self.exclusions.contains(module) {
                    ExclusionReason::Explicit
                } else {
                    ExclusionReason::Unreachable
                },
            })
            .collect();

        resolved_requirements.sort_unstable();
        resolved_contributions.sort_unstable();

        Ok(ProcessProjection {
            application,
            process,
            roots,
            included_modules,
            excluded_modules,
            exclusions,
            requirements: resolved_requirements,
            contributions: resolved_contributions,
        })
    }

    /// Resolves one process and consumes the mutable blueprint at the freeze boundary.
    pub fn freeze(self, process: ProcessId) -> Result<FrozenProcess, ProjectionError> {
        Ok(self.freeze_processes(&[process])?.remove(0))
    }

    /// Resolves selected processes from one blueprint and consumes it at freeze.
    pub fn freeze_processes(
        self,
        processes: &[ProcessId],
    ) -> Result<Vec<FrozenProcess>, ProjectionError> {
        processes
            .iter()
            .map(|process| self.project(*process).map(FrozenProcess::new))
            .collect()
    }

    fn ensure_module(
        &self,
        application: ApplicationId,
        process: ProcessId,
        module: ModuleId,
    ) -> Result<(), ProjectionError> {
        if self.modules.contains(&module) {
            Ok(())
        } else {
            Err(ProjectionError::MissingModule {
                application,
                process,
                module,
            })
        }
    }
}

fn find_cycle(edges: &[(ModuleId, ModuleId)]) -> Option<Vec<ModuleId>> {
    fn visit(
        node: ModuleId,
        graph: &BTreeMap<ModuleId, Vec<ModuleId>>,
        active: &mut Vec<ModuleId>,
        complete: &mut BTreeSet<ModuleId>,
    ) -> Option<Vec<ModuleId>> {
        if let Some(start) = active.iter().position(|candidate| *candidate == node) {
            let mut cycle = active[start..].to_vec();
            cycle.push(node);
            return Some(cycle);
        }
        if !complete.insert(node) {
            return None;
        }
        active.push(node);
        for next in graph.get(&node).into_iter().flatten() {
            if let Some(cycle) = visit(*next, graph, active, complete) {
                return Some(cycle);
            }
        }
        active.pop();
        None
    }

    let mut graph = BTreeMap::<ModuleId, Vec<ModuleId>>::new();
    for (from, to) in edges {
        graph.entry(*from).or_default().push(*to);
    }
    for neighbors in graph.values_mut() {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    let mut complete = BTreeSet::new();
    let mut active = Vec::new();
    for node in graph.keys().copied() {
        if let Some(cycle) = visit(node, &graph, &mut active, &mut complete) {
            return Some(cycle);
        }
    }
    None
}

/// Why a module is included in a process projection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InclusionReason {
    /// The module provides an execution root of the process.
    ExecutionRoot {
        /// The execution represented by the root.
        execution: ExecutionId,
    },
    /// The module provides a capability required by an included consumer.
    CapabilityProvider {
        /// The module requiring this provider.
        required_by: ModuleId,
        /// The required capability identity.
        capability: CapabilityId,
        /// The capability qualifier, if present.
        qualifier: Option<QualifierId>,
        /// How this provider was selected.
        selection: ProviderSelection,
        /// The original provider when this one replaced it.
        replaced: Option<ModuleId>,
    },
    /// The module contributes to a target consumed by an included module.
    Contribution {
        /// The module consuming the target.
        consumed_by: ModuleId,
        /// The consumed target identity.
        target: ContributionTargetId,
        /// The target qualifier.
        qualifier: QualifierId,
        /// The contribution kind identity.
        contribution: ContributionId,
    },
}

/// How a resolved provider was selected for a requirement.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProviderSelection {
    /// One provider uniquely matched the requirement.
    Unique,
    /// A declared default selected the provider.
    Default,
    /// An explicit selection overrode defaults or automatic choice.
    Explicit,
    /// The optional requirement was not activated.
    OptionalAbsent,
}

/// Why a declared module is absent from a process projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExclusionReason {
    /// The blueprint explicitly excluded the module.
    Explicit,
    /// No selected root or relation reached the module.
    Unreachable,
}

/// An omitted module and its projection-specific exclusion reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExcludedModule {
    module: ModuleId,
    reason: ExclusionReason,
}

impl ExcludedModule {
    /// Returns the omitted module identity.
    pub const fn module(&self) -> ModuleId {
        self.module
    }

    /// Returns why the module is absent.
    pub const fn reason(&self) -> ExclusionReason {
        self.reason
    }
}

/// A capability requirement and its selected provider in a process projection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResolvedRequirement {
    consumer: ModuleId,
    capability: CapabilityId,
    qualifier: Option<QualifierId>,
    provider: Option<ModuleId>,
    optional: bool,
    selection: ProviderSelection,
    replaced: Option<ModuleId>,
}

impl ResolvedRequirement {
    /// Returns the module that owns the requirement.
    pub const fn consumer(&self) -> ModuleId {
        self.consumer
    }
    /// Returns the required capability identity.
    pub const fn capability(&self) -> CapabilityId {
        self.capability
    }
    /// Returns the required qualifier, if any.
    pub const fn qualifier(&self) -> Option<QualifierId> {
        self.qualifier
    }
    /// Returns the selected provider, or `None` for an inactive optional requirement.
    pub const fn provider(&self) -> Option<ModuleId> {
        self.provider
    }
    /// Returns whether the declared requirement is optional.
    pub const fn optional(&self) -> bool {
        self.optional
    }
    /// Returns how the provider was selected.
    pub const fn selection(&self) -> ProviderSelection {
        self.selection
    }
    /// Returns the provider identity replaced by the selected provider, if any.
    pub const fn replaced_provider(&self) -> Option<ModuleId> {
        self.replaced
    }
}

/// A matching target contribution in the selected process projection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResolvedContribution {
    consumer: ModuleId,
    contributor: ModuleId,
    target: ContributionTargetId,
    qualifier: QualifierId,
    contribution: ContributionId,
}

impl ResolvedContribution {
    /// Returns the module consuming the target.
    pub const fn consumer(&self) -> ModuleId {
        self.consumer
    }
    /// Returns the contributing module.
    pub const fn contributor(&self) -> ModuleId {
        self.contributor
    }
    /// Returns the target identity.
    pub const fn target(&self) -> ContributionTargetId {
        self.target
    }
    /// Returns the target qualifier.
    pub const fn qualifier(&self) -> QualifierId {
        self.qualifier
    }
    /// Returns the contribution kind identity.
    pub const fn contribution(&self) -> ContributionId {
        self.contribution
    }
}

/// One reachable module and the deterministic path that made it reachable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncludedModule {
    module: ModuleId,
    path: Vec<ModuleId>,
    reason: InclusionReason,
}

impl IncludedModule {
    /// Returns the included module identity.
    pub const fn module(&self) -> ModuleId {
        self.module
    }

    /// Returns the root-to-module inclusion path.
    pub fn path(&self) -> &[ModuleId] {
        &self.path
    }

    /// Returns the edge that included this module or the root execution.
    pub const fn reason(&self) -> InclusionReason {
        self.reason
    }
}

/// A derived process projection. Its private structure cannot be mutated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessProjection {
    application: ApplicationId,
    process: ProcessId,
    roots: Vec<ExecutionId>,
    included_modules: Vec<IncludedModule>,
    excluded_modules: Vec<ModuleId>,
    exclusions: Vec<ExcludedModule>,
    requirements: Vec<ResolvedRequirement>,
    contributions: Vec<ResolvedContribution>,
}

impl ProcessProjection {
    /// Returns the application blueprint identity.
    pub const fn application(&self) -> ApplicationId {
        self.application
    }

    /// Returns the selected process identity.
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    /// Returns the process's execution roots.
    pub fn roots(&self) -> &[ExecutionId] {
        &self.roots
    }

    /// Returns reachable modules with provenance paths.
    pub fn included_modules(&self) -> &[IncludedModule] {
        &self.included_modules
    }

    /// Returns declared modules excluded from this process projection.
    pub fn excluded_modules(&self) -> &[ModuleId] {
        &self.excluded_modules
    }

    /// Returns omitted modules with their exclusion reasons.
    pub fn exclusions(&self) -> &[ExcludedModule] {
        &self.exclusions
    }

    /// Returns requirements resolved for included modules.
    pub fn requirements(&self) -> &[ResolvedRequirement] {
        &self.requirements
    }

    /// Returns consumed contributions in this process projection.
    pub fn contributions(&self) -> &[ResolvedContribution] {
        &self.contributions
    }

    /// Returns whether a module is reachable in this projection.
    pub fn includes(&self, module: ModuleId) -> bool {
        self.included_modules
            .iter()
            .any(|entry| entry.module == module)
    }
}

/// Compact structural input for initializing a selected runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeProjection {
    modules: Vec<ModuleId>,
    contributions: Vec<ResolvedContribution>,
}

impl RuntimeProjection {
    /// Returns the selected module identities in deterministic order.
    pub fn modules(&self) -> &[ModuleId] {
        &self.modules
    }

    /// Returns target contribution work assigned to the consuming modules.
    pub fn contributions(&self) -> &[ResolvedContribution] {
        &self.contributions
    }
}

/// A structurally immutable projection produced at the composition freeze boundary.
///
/// ```compile_fail
/// use rustclamp_core::ModuleId;
/// use rustclamp_kernel::FrozenProcess;
/// fn mutate(frozen: &mut FrozenProcess) {
///     frozen.add_module(ModuleId::new("late.module"));
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenProcess {
    runtime: RuntimeProjection,
    inspection: ProcessProjection,
}

impl FrozenProcess {
    fn new(inspection: ProcessProjection) -> Self {
        let runtime = RuntimeProjection {
            modules: inspection
                .included_modules()
                .iter()
                .map(IncludedModule::module)
                .collect(),
            contributions: inspection.contributions.clone(),
        };
        Self {
            runtime,
            inspection,
        }
    }

    /// Returns the compact runtime projection derived from the resolved model.
    pub const fn runtime(&self) -> &RuntimeProjection {
        &self.runtime
    }

    /// Returns immutable resolution metadata for inspection.
    pub const fn inspection(&self) -> &ProcessProjection {
        &self.inspection
    }

    /// Resolves `consumer`'s unqualified requirement for `C` to the value of the
    /// provider this process selected at freeze time.
    ///
    /// The blueprint stays the single declaration of the edge: the caller supplies
    /// only the constructed values. An undeclared or inactive optional requirement
    /// fails with [`CompositionErrorKind::UndeclaredRequirement`]; a selected provider
    /// missing from `provisions` fails with
    /// [`CompositionErrorKind::ProviderSelectionUnavailable`].
    pub fn resolve<'value, C: Capability>(
        &self,
        consumer: ModuleId,
        provisions: &[Provision<'value, C>],
    ) -> Result<&'value C::Value, CompositionError> {
        // ponytail: unqualified only; add resolve_qualified when a process needs it.
        let provider = self
            .inspection
            .requirements()
            .iter()
            .find(|requirement| {
                requirement.consumer == consumer
                    && requirement.capability == C::ID
                    && requirement.qualifier.is_none()
            })
            .and_then(|requirement| requirement.provider);
        match provider {
            Some(provider) => Resolver::resolve(consumer, provisions, Some(provider)),
            None => Err(Resolver::error(
                CompositionErrorKind::UndeclaredRequirement,
                consumer,
                C::ID,
                None,
                provisions
                    .iter()
                    .map(|provision| (provision.module, provision.value)),
            )),
        }
    }

    /// Builds target `T` (qualifier `Q`) from the contributions this process selected.
    ///
    /// The blueprint's `add_contribution` edges stay the single declaration:
    /// contributions from modules outside this process are dropped, and the rest
    /// must match the frozen edges exactly — an included module supplying an
    /// undeclared contribution, or a declared contributor supplying none, fails.
    pub fn compose<T: ContributionTarget, Q: Qualifier>(
        &self,
        target: &T,
        contributions: Vec<(ModuleId, T::Contribution)>,
    ) -> Result<T::Runtime, ComposeError<T::Error>> {
        let declared = self
            .inspection
            .contributions()
            .iter()
            .filter(|edge| {
                edge.target == T::ID
                    && edge.qualifier == Q::ID
                    && edge.contribution == T::Contribution::ID
            })
            .map(|edge| edge.contributor)
            .collect::<BTreeSet<_>>();
        let selected = contributions
            .into_iter()
            .filter(|(module, _)| self.inspection.includes(*module))
            .collect::<Vec<_>>();
        if let Some((module, _)) = selected
            .iter()
            .find(|(module, _)| !declared.contains(module))
        {
            return Err(ComposeError::UndeclaredContribution {
                target: T::ID,
                qualifier: Q::ID,
                contributor: *module,
            });
        }
        if let Some(module) = declared
            .iter()
            .find(|declared| !selected.iter().any(|(module, _)| module == *declared))
        {
            return Err(ComposeError::MissingContribution {
                target: T::ID,
                qualifier: Q::ID,
                contributor: *module,
            });
        }
        target.build(&selected).map_err(ComposeError::Target)
    }
}

/// A structured failure while deriving a selected process projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionError {
    /// The application has no declaration for the requested process.
    MissingProcess {
        /// The application identity.
        application: ApplicationId,
        /// The requested process identity.
        process: ProcessId,
    },
    /// The process is declared without an execution root.
    EmptyProcessRoots {
        /// The application identity.
        application: ApplicationId,
        /// The process identity with no roots.
        process: ProcessId,
    },
    /// A process root refers to an undeclared execution.
    MissingExecution {
        /// The application identity.
        application: ApplicationId,
        /// The selected process identity.
        process: ProcessId,
        /// The undeclared root execution.
        execution: ExecutionId,
    },
    /// A root or reachable edge refers to an undeclared module.
    MissingModule {
        /// The application identity.
        application: ApplicationId,
        /// The selected process identity.
        process: ProcessId,
        /// The undeclared module identity.
        module: ModuleId,
    },
    /// An included required capability has no provider.
    MissingProvider {
        /// Application identity.
        application: ApplicationId,
        /// Projected process identity.
        process: ProcessId,
        /// Requiring module.
        required_by: ModuleId,
        /// Required capability.
        capability: CapabilityId,
        /// Required qualifier, if any.
        qualifier: Option<QualifierId>,
    },
    /// An included requirement has multiple providers without an explicit selection.
    AmbiguousProvider {
        /// Application identity.
        application: ApplicationId,
        /// Projected process identity.
        process: ProcessId,
        /// Requiring module.
        required_by: ModuleId,
        /// Required capability.
        capability: CapabilityId,
        /// Required qualifier, if any.
        qualifier: Option<QualifierId>,
        /// Sorted matching provider modules.
        candidates: Vec<ModuleId>,
    },
    /// A selected provider is not declared for the requirement.
    UnavailableProvider {
        /// Application identity.
        application: ApplicationId,
        /// Projected process identity.
        process: ProcessId,
        /// Requiring module.
        required_by: ModuleId,
        /// Required capability.
        capability: CapabilityId,
        /// Required qualifier, if any.
        qualifier: Option<QualifierId>,
        /// Selected provider module.
        provider: ModuleId,
    },
    /// An included requirement selected an excluded provider.
    ExcludedProvider {
        /// Application identity.
        application: ApplicationId,
        /// Projected process identity.
        process: ProcessId,
        /// Requiring module.
        required_by: ModuleId,
        /// Required capability.
        capability: CapabilityId,
        /// Required qualifier, if any.
        qualifier: Option<QualifierId>,
        /// Excluded provider module.
        provider: ModuleId,
    },
    /// An execution root was explicitly excluded.
    ExcludedRoot {
        /// Application identity.
        application: ApplicationId,
        /// Projected process identity.
        process: ProcessId,
        /// Excluded root module.
        module: ModuleId,
    },
    /// A cycle exists in the selected process dependency graph.
    DependencyCycle {
        /// Application identity.
        application: ApplicationId,
        /// Projected process identity.
        process: ProcessId,
        /// Closed cycle path, with the first module repeated at the end.
        path: Vec<ModuleId>,
    },
    /// A reachable module declares a required contribution without a consumer.
    OrphanContribution {
        /// Application identity.
        application: ApplicationId,
        /// Projected process identity.
        process: ProcessId,
        /// Module declaring the contribution.
        contributor: ModuleId,
        /// Contribution target identity.
        target: ContributionTargetId,
        /// Contribution target qualifier.
        qualifier: QualifierId,
        /// Contribution kind identity.
        contribution: ContributionId,
    },
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (application, process) = match self {
            Self::MissingProcess {
                application,
                process,
                ..
            }
            | Self::EmptyProcessRoots {
                application,
                process,
                ..
            }
            | Self::MissingExecution {
                application,
                process,
                ..
            }
            | Self::MissingModule {
                application,
                process,
                ..
            }
            | Self::MissingProvider {
                application,
                process,
                ..
            }
            | Self::AmbiguousProvider {
                application,
                process,
                ..
            }
            | Self::UnavailableProvider {
                application,
                process,
                ..
            }
            | Self::ExcludedProvider {
                application,
                process,
                ..
            }
            | Self::ExcludedRoot {
                application,
                process,
                ..
            }
            | Self::DependencyCycle {
                application,
                process,
                ..
            }
            | Self::OrphanContribution {
                application,
                process,
                ..
            } => (application.as_str(), process.as_str()),
        };
        write!(formatter, "process '{application}/{process}': ")?;
        match self {
            Self::MissingProcess { .. } => formatter.write_str("process is not declared"),
            Self::EmptyProcessRoots { .. } => formatter.write_str("process has no execution root"),
            Self::MissingExecution { execution, .. } => {
                write!(
                    formatter,
                    "execution '{}' is not declared",
                    execution.as_str()
                )
            }
            Self::MissingModule { module, .. } => {
                write!(formatter, "module '{}' is not declared", module.as_str())
            }
            Self::MissingProvider {
                required_by,
                capability,
                qualifier,
                ..
            } => {
                write!(
                    formatter,
                    "no provider for capability '{}'{} required by module '{}'",
                    capability.as_str(),
                    Qualified(*qualifier),
                    required_by.as_str()
                )
            }
            Self::AmbiguousProvider {
                required_by,
                capability,
                qualifier,
                candidates,
                ..
            } => {
                write!(
                    formatter,
                    "ambiguous providers for capability '{}'{} required by module '{}': {}",
                    capability.as_str(),
                    Qualified(*qualifier),
                    required_by.as_str(),
                    candidates
                        .iter()
                        .map(|module| module.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Self::UnavailableProvider {
                required_by,
                capability,
                qualifier,
                provider,
                ..
            } => {
                write!(
                    formatter,
                    "selected provider '{}' does not provide capability '{}'{} required by module '{}'",
                    provider.as_str(),
                    capability.as_str(),
                    Qualified(*qualifier),
                    required_by.as_str()
                )
            }
            Self::ExcludedProvider {
                required_by,
                capability,
                qualifier,
                provider,
                ..
            } => {
                write!(
                    formatter,
                    "selected provider '{}' for capability '{}'{} required by module '{}' is excluded",
                    provider.as_str(),
                    capability.as_str(),
                    Qualified(*qualifier),
                    required_by.as_str()
                )
            }
            Self::ExcludedRoot { module, .. } => {
                write!(formatter, "root module '{}' is excluded", module.as_str())
            }
            Self::DependencyCycle { path, .. } => {
                write!(
                    formatter,
                    "dependency cycle: {}",
                    path.iter()
                        .map(|module| module.as_str())
                        .collect::<Vec<_>>()
                        .join(" -> ")
                )
            }
            Self::OrphanContribution {
                contributor,
                target,
                qualifier,
                contribution,
                ..
            } => {
                write!(
                    formatter,
                    "required contribution '{}' from module '{}' has no consumer for target '{}' qualified as '{}'",
                    contribution.as_str(),
                    contributor.as_str(),
                    target.as_str(),
                    qualifier.as_str()
                )
            }
        }
    }
}

impl Error for ProjectionError {}

/// Renders an optional qualifier suffix in diagnostics.
struct Qualified(Option<QualifierId>);

impl fmt::Display for Qualified {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(qualifier) => write!(formatter, " qualified as '{}'", qualifier.as_str()),
            None => Ok(()),
        }
    }
}

/// Caller-owned declarations for one typed contribution target and qualifier.
pub struct TargetComposition<T: ContributionTarget, Q: Qualifier> {
    contributions: Vec<(ModuleId, T::Contribution)>,
    target: PhantomData<(T, Q)>,
}

impl<T: ContributionTarget, Q: Qualifier> TargetComposition<T, Q> {
    /// Creates a target composition from contributor identities and declarations.
    pub fn new(contributions: Vec<(ModuleId, T::Contribution)>) -> Self {
        Self {
            contributions,
            target: PhantomData,
        }
    }

    /// Consumes declarations into the selected target's runtime form.
    ///
    /// If no target is selected, required declarations produce a structured
    /// unconsumed-contribution error. Optional declarations are discarded. An
    /// empty composition without a target is a no-op.
    pub fn build(
        self,
        target: Option<&T>,
    ) -> Result<Option<T::Runtime>, TargetCompositionError<T::Error>> {
        let Some(target) = target else {
            let mut contributors = self
                .contributions
                .iter()
                .filter(|_| T::Contribution::REQUIRED)
                .map(|(module, _)| *module)
                .collect::<Vec<_>>();
            if contributors.is_empty() {
                return Ok(None);
            }
            contributors.sort_unstable();
            return Err(TargetCompositionError::UnconsumedRequired {
                target: T::ID,
                contribution: T::Contribution::ID,
                qualifier: Q::ID,
                contributors,
            });
        };

        target
            .build(&self.contributions)
            .map(Some)
            .map_err(TargetCompositionError::Target)
    }
}

/// A target-owned build failure or an unconsumed required declaration.
#[derive(Debug, Eq, PartialEq)]
pub enum TargetCompositionError<E> {
    /// The active composition has required declarations but no matching target.
    UnconsumedRequired {
        /// The target that was expected to consume the declarations.
        target: ContributionTargetId,
        /// The contribution kind that was left unconsumed.
        contribution: ContributionId,
        /// The qualifier whose active composition contains the declarations.
        qualifier: QualifierId,
        /// Modules that contributed required declarations.
        contributors: Vec<ModuleId>,
    },
    /// The selected target rejected the declarations.
    Target(E),
}

impl<E: fmt::Display> fmt::Display for TargetCompositionError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnconsumedRequired {
                target,
                contribution,
                qualifier,
                contributors,
            } => write!(
                formatter,
                "required contribution '{}' qualified as '{}' has no target '{}'; contributors: {}",
                contribution.as_str(),
                qualifier.as_str(),
                target.as_str(),
                contributors
                    .iter()
                    .map(|module| module.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Target(error) => write!(formatter, "target rejected declarations: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for TargetCompositionError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Target(error) => Some(error),
            Self::UnconsumedRequired { .. } => None,
        }
    }
}

/// A mismatch between supplied contributions and the frozen process, or a target failure.
#[derive(Debug, Eq, PartialEq)]
pub enum ComposeError<E> {
    /// An included module supplied a contribution the frozen process does not declare.
    UndeclaredContribution {
        /// The target being composed.
        target: ContributionTargetId,
        /// The target qualifier.
        qualifier: QualifierId,
        /// The module whose contribution has no blueprint edge.
        contributor: ModuleId,
    },
    /// A contributor declared in the frozen process supplied no contribution.
    MissingContribution {
        /// The target being composed.
        target: ContributionTargetId,
        /// The target qualifier.
        qualifier: QualifierId,
        /// The declared module that supplied nothing.
        contributor: ModuleId,
    },
    /// The target rejected the selected contributions.
    Target(E),
}

impl<E: fmt::Display> fmt::Display for ComposeError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndeclaredContribution {
                target,
                qualifier,
                contributor,
            } => write!(
                formatter,
                "module '{}' contributes to target '{}' qualified as '{}' without a blueprint edge in this process",
                contributor.as_str(),
                target.as_str(),
                qualifier.as_str()
            ),
            Self::MissingContribution {
                target,
                qualifier,
                contributor,
            } => write!(
                formatter,
                "module '{}' is declared to contribute to target '{}' qualified as '{}' but supplied nothing",
                contributor.as_str(),
                target.as_str(),
                qualifier.as_str()
            ),
            Self::Target(error) => write!(formatter, "target rejected contributions: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for ComposeError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Target(error) => Some(error),
            Self::UndeclaredContribution { .. } | Self::MissingContribution { .. } => None,
        }
    }
}

/// A capability implementation associated with the module that supplies it.
///
/// This is a graph relationship, not a provider base class. The value may be
/// borrowed from an object owned and constructed by the application.
pub struct Provision<'a, C: Capability> {
    module: ModuleId,
    value: &'a C::Value,
}

impl<'a, C: Capability> Provision<'a, C> {
    /// Associates a module identity with a value implementing capability `C`.
    pub fn new(module: ModuleId, value: &'a C::Value) -> Self {
        Self { module, value }
    }

    /// Creates a provision from a module's additive [`Provides`] contract.
    pub fn from_module<M: Provides<C>>(module: &'a M) -> Self {
        Self::new(<M as Module>::ID, module.provided_value())
    }

    /// Returns the module identity associated with this provision.
    pub const fn module(&self) -> ModuleId {
        self.module
    }

    /// Returns the provided capability value.
    pub const fn value(&self) -> &'a C::Value {
        self.value
    }
}

/// One module's requirement for a single typed capability.
pub struct CapabilityRequirement<C: Capability> {
    required_by: ModuleId,
    selected: Option<ModuleId>,
    capability: PhantomData<C>,
}

impl<C: Capability> Copy for CapabilityRequirement<C> {}

impl<C: Capability> Clone for CapabilityRequirement<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Capability> CapabilityRequirement<C> {
    /// Declares a requirement with an optional explicit provider selection.
    pub const fn new(required_by: ModuleId, selected: Option<ModuleId>) -> Self {
        Self {
            required_by,
            selected,
            capability: PhantomData,
        }
    }

    /// Creates a requirement from a module's additive [`Requires`] contract.
    pub fn from_module<M: Requires<C>>() -> Self {
        Self::new(<M as Module>::ID, M::SELECTED_PROVIDER)
    }

    /// Returns the module that owns this requirement.
    pub const fn required_by(self) -> ModuleId {
        self.required_by
    }

    /// Returns the explicitly selected provider, if one was declared.
    pub const fn selected_provider(self) -> Option<ModuleId> {
        self.selected
    }
}

/// A caller-owned, single-capability composition snapshot.
///
/// This prototype stores declarations only; it does not construct modules or
/// keep global state. Edits return a new snapshot so callers can validate the
/// resulting graph before adopting it.
pub struct CapabilityComposition<'a, C: Capability> {
    provisions: Vec<Provision<'a, C>>,
    requirements: Vec<CapabilityRequirement<C>>,
}

impl<'a, C: Capability> CapabilityComposition<'a, C> {
    /// Creates a composition snapshot from providers and consumer requirements.
    pub fn new(
        provisions: Vec<Provision<'a, C>>,
        requirements: Vec<CapabilityRequirement<C>>,
    ) -> Self {
        Self {
            provisions,
            requirements,
        }
    }

    /// Removes a module's declarations and revalidates the remaining graph.
    ///
    /// Requirements owned by other modules remain, even when the removed module
    /// was their selected provider; validation then reports the broken edge.
    pub fn without_module(&self, module: ModuleId) -> Self {
        Self {
            provisions: self
                .provisions
                .iter()
                .filter(|provision| provision.module != module)
                .map(|provision| Provision::new(provision.module, provision.value))
                .collect(),
            requirements: self
                .requirements
                .iter()
                .copied()
                .filter(|requirement| requirement.required_by != module)
                .collect(),
        }
    }

    /// Returns the active consumer requirements in this snapshot.
    pub fn requirements(&self) -> &[CapabilityRequirement<C>] {
        &self.requirements
    }

    /// Replaces one provider for this capability while retaining module
    /// requirements and redirecting explicit selections to the replacement.
    /// Returns `None` when `replaced` does not currently provide this capability.
    pub fn replace_provider(
        &self,
        replaced: ModuleId,
        replacement: Provision<'a, C>,
    ) -> Option<Self> {
        if !self
            .provisions
            .iter()
            .any(|provision| provision.module == replaced)
        {
            return None;
        }

        let mut provisions = self
            .provisions
            .iter()
            .filter(|provision| provision.module != replaced)
            .map(|provision| Provision::new(provision.module, provision.value))
            .collect::<Vec<_>>();
        let replacement_module = replacement.module;
        provisions.push(replacement);
        let requirements = self
            .requirements
            .iter()
            .map(|requirement| {
                CapabilityRequirement::new(
                    requirement.required_by,
                    (requirement.selected == Some(replaced))
                        .then_some(replacement_module)
                        .or(requirement.selected),
                )
            })
            .collect();

        Some(Self::new(provisions, requirements))
    }

    /// Resolves one declared requirement against this snapshot.
    pub fn resolve_requirement(
        &self,
        requirement: CapabilityRequirement<C>,
    ) -> Result<&'a C::Value, CompositionError> {
        Resolver::resolve::<C>(
            requirement.required_by,
            &self.provisions,
            requirement.selected,
        )
    }

    /// Revalidates every consumer requirement in this snapshot.
    pub fn validate(&self) -> Result<(), CompositionError> {
        for requirement in &self.requirements {
            self.resolve_requirement(*requirement)?;
        }
        Ok(())
    }
}

/// A capability provision distinguished by a compile-time qualifier type.
pub struct QualifiedProvision<'a, C: Capability, Q: Qualifier> {
    module: ModuleId,
    value: &'a C::Value,
    qualifier: PhantomData<Q>,
}

impl<'a, C: Capability, Q: Qualifier> QualifiedProvision<'a, C, Q> {
    /// Associates a module and a typed qualifier with a capability value.
    pub fn new(module: ModuleId, value: &'a C::Value) -> Self {
        Self {
            module,
            value,
            qualifier: PhantomData,
        }
    }

    /// Returns the module identity associated with this provision.
    pub const fn module(&self) -> ModuleId {
        self.module
    }
}

/// The structured category of a capability composition failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositionErrorKind {
    /// No module provides a required capability.
    MissingProvider,
    /// More than one module provides the capability and none was selected.
    AmbiguousProviders,
    /// The selected module is not among the available provisions.
    ProviderSelectionUnavailable,
    /// The frozen process has no active requirement for this consumer and capability.
    UndeclaredRequirement,
}

/// A composition failure with stable identities and candidate context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionError {
    kind: CompositionErrorKind,
    required_by: ModuleId,
    capability: CapabilityId,
    qualifier: Option<QualifierId>,
    candidates: Vec<ModuleId>,
}

impl CompositionError {
    fn new(
        kind: CompositionErrorKind,
        required_by: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        candidates: Vec<ModuleId>,
    ) -> Self {
        Self {
            kind,
            required_by,
            capability,
            qualifier,
            candidates,
        }
    }

    /// Returns the structured failure category.
    pub const fn kind(&self) -> CompositionErrorKind {
        self.kind
    }

    /// Returns the module whose requirement could not be resolved.
    pub const fn required_by(&self) -> ModuleId {
        self.required_by
    }

    /// Returns the stable identity of the missing or ambiguous capability.
    pub const fn capability(&self) -> CapabilityId {
        self.capability
    }

    /// Returns the qualifier identity when the failed requirement was qualified.
    pub const fn qualifier(&self) -> Option<QualifierId> {
        self.qualifier
    }

    /// Returns candidate module identities in stable sorted order.
    pub fn candidates(&self) -> &[ModuleId] {
        &self.candidates
    }
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} for capability '{}' required by module '{}'",
            self.kind,
            self.capability.as_str(),
            self.required_by.as_str()
        )?;
        if let Some(qualifier) = self.qualifier {
            write!(formatter, " qualified as '{}'", qualifier.as_str())?;
        }
        if !self.candidates.is_empty() {
            formatter.write_str("; candidates: ")?;
            for (index, candidate) in self.candidates.iter().enumerate() {
                if index > 0 {
                    formatter.write_str(", ")?;
                }
                formatter.write_str(candidate.as_str())?;
            }
        }
        Ok(())
    }
}

impl Error for CompositionError {}

/// A directed construction dependency from a module to a module it needs.
///
/// An edge `consumer -> dependency` means the dependency must be constructed
/// before the consumer. The graph only validates construction order; it does
/// not construct modules or support lazy/proxy edges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructionDependency {
    consumer: ModuleId,
    dependency: ModuleId,
}

impl ConstructionDependency {
    /// Declares that `consumer` requires `dependency` during construction.
    pub const fn new(consumer: ModuleId, dependency: ModuleId) -> Self {
        Self {
            consumer,
            dependency,
        }
    }

    /// Returns the module that has the dependency.
    pub const fn consumer(self) -> ModuleId {
        self.consumer
    }

    /// Returns the module required during construction.
    pub const fn dependency(self) -> ModuleId {
        self.dependency
    }
}

/// A caller-owned set of construction dependencies for cycle validation.
#[derive(Clone, Debug, Default)]
pub struct ConstructionGraph {
    dependencies: Vec<ConstructionDependency>,
}

impl ConstructionGraph {
    /// Creates a graph from module-to-module construction dependencies.
    pub fn new(dependencies: Vec<ConstructionDependency>) -> Self {
        Self { dependencies }
    }

    /// Returns the declared construction dependencies.
    pub fn dependencies(&self) -> &[ConstructionDependency] {
        &self.dependencies
    }

    /// Rejects a construction cycle and returns its closed module path.
    ///
    /// For example, `A -> B -> C -> A` reports `[A, B, C, A]`. Traversal is
    /// sorted by stable module identity, so the selected diagnostic is
    /// independent of dependency registration order.
    pub fn validate(&self) -> Result<(), ConstructionCycle> {
        let mut adjacency = BTreeMap::<ModuleId, Vec<ModuleId>>::new();
        for edge in &self.dependencies {
            adjacency
                .entry(edge.consumer)
                .or_default()
                .push(edge.dependency);
            adjacency.entry(edge.dependency).or_default();
        }

        // Deduplicate and sort once so traversal and diagnostics are stable.
        for dependencies in adjacency.values_mut() {
            dependencies.sort_unstable();
            dependencies.dedup();
        }

        let mut states = BTreeMap::<ModuleId, VisitState>::new();
        let mut path = Vec::new();
        for module in adjacency.keys().copied() {
            if states
                .get(&module)
                .copied()
                .unwrap_or(VisitState::Unvisited)
                == VisitState::Unvisited
            {
                states.insert(module, VisitState::Visiting);
                path.push(module);
                let mut stack = vec![(module, 0usize)];

                while let Some((current, next_index)) = stack.last_mut() {
                    let dependencies = &adjacency[current];
                    if *next_index == dependencies.len() {
                        let (finished, _) = stack.pop().expect("stack is non-empty");
                        path.pop();
                        states.insert(finished, VisitState::Visited);
                        continue;
                    }

                    let dependency = dependencies[*next_index];
                    *next_index += 1;
                    match states
                        .get(&dependency)
                        .copied()
                        .unwrap_or(VisitState::Unvisited)
                    {
                        VisitState::Unvisited => {
                            states.insert(dependency, VisitState::Visiting);
                            path.push(dependency);
                            stack.push((dependency, 0));
                        }
                        VisitState::Visiting => {
                            let start = path
                                .iter()
                                .position(|item| *item == dependency)
                                .expect("visiting modules are in the active path");
                            let mut cycle = path[start..].to_vec();
                            cycle.push(dependency);
                            return Err(ConstructionCycle { path: cycle });
                        }
                        VisitState::Visited => {}
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

/// A construction cycle with the repeated start module closing its path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstructionCycle {
    path: Vec<ModuleId>,
}

impl ConstructionCycle {
    /// Returns the dependency path, including the repeated start module.
    pub fn path(&self) -> &[ModuleId] {
        &self.path
    }
}

impl fmt::Display for ConstructionCycle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("construction cycle: ")?;
        for (index, module) in self.path.iter().enumerate() {
            if index > 0 {
                formatter.write_str(" -> ")?;
            }
            formatter.write_str(module.as_str())?;
        }
        Ok(())
    }
}

impl Error for ConstructionCycle {}

/// Resolves one typed requirement from the provisions declared for it.
pub struct Resolver;

impl Resolver {
    /// Resolves a unique provision, or the provision named by `selected`.
    ///
    /// Registration order never selects a winner. Multiple candidates without
    /// explicit selection produce a structured ambiguity error. No global
    /// registry, runtime type map, or allocation is needed on the success path.
    pub fn resolve<'value, C: Capability>(
        required_by: ModuleId,
        provisions: &[Provision<'value, C>],
        selected: Option<ModuleId>,
    ) -> Result<&'value C::Value, CompositionError> {
        let capability = C::ID;
        Self::resolve_candidates(
            required_by,
            capability,
            None,
            provisions
                .iter()
                .map(|provision| (provision.module, provision.value)),
            provisions.len(),
            selected,
        )
    }

    /// Resolves an optional requirement without installing a default provider.
    /// Zero candidates returns `Ok(None)`, one returns its value, and multiple
    /// candidates remain ambiguous.
    pub fn resolve_optional<'value, C: Capability>(
        required_by: ModuleId,
        provisions: &[Provision<'value, C>],
    ) -> Result<Option<&'value C::Value>, CompositionError> {
        match provisions {
            [] => Ok(None),
            [provision] => Ok(Some(provision.value)),
            _ => Err(Self::error(
                CompositionErrorKind::AmbiguousProviders,
                required_by,
                C::ID,
                None,
                provisions
                    .iter()
                    .map(|provision| (provision.module, provision.value)),
            )),
        }
    }

    /// Returns every provider for a many-valued requirement.
    ///
    /// Results are sorted by module identity for stable inspection only. This
    /// order does not define provider execution or dependency order.
    pub fn resolve_many<'value, C: Capability>(
        provisions: &[Provision<'value, C>],
    ) -> Vec<Provision<'value, C>> {
        let mut resolved = provisions
            .iter()
            .map(|provision| Provision::new(provision.module, provision.value))
            .collect::<Vec<_>>();
        resolved.sort_unstable_by_key(Provision::module);
        resolved
    }

    /// Resolves a provision for one typed qualifier, excluding other qualifiers
    /// from candidate counting and diagnostics.
    pub fn resolve_qualified<'value, C: Capability, Q: Qualifier>(
        required_by: ModuleId,
        provisions: &[QualifiedProvision<'value, C, Q>],
        selected: Option<ModuleId>,
    ) -> Result<&'value C::Value, CompositionError> {
        Self::resolve_candidates(
            required_by,
            C::ID,
            Some(Q::ID),
            provisions
                .iter()
                .map(|provision| (provision.module, provision.value)),
            provisions.len(),
            selected,
        )
    }

    fn resolve_candidates<'value, V: ?Sized + 'value>(
        required_by: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        mut provisions: impl Iterator<Item = (ModuleId, &'value V)> + Clone,
        count: usize,
        selected: Option<ModuleId>,
    ) -> Result<&'value V, CompositionError> {
        if let Some(selected) = selected {
            let mut matches = provisions.clone().filter(|(module, _)| *module == selected);
            if let Some((_, value)) = matches.next() {
                if matches.next().is_none() {
                    return Ok(value);
                }
                return Err(Self::error(
                    CompositionErrorKind::AmbiguousProviders,
                    required_by,
                    capability,
                    qualifier,
                    provisions,
                ));
            }
            return Err(Self::error(
                CompositionErrorKind::ProviderSelectionUnavailable,
                required_by,
                capability,
                qualifier,
                provisions,
            ));
        }

        match count {
            0 => Err(Self::error(
                CompositionErrorKind::MissingProvider,
                required_by,
                capability,
                qualifier,
                provisions,
            )),
            1 => Ok(provisions.next().expect("count checked").1),
            _ => Err(Self::error(
                CompositionErrorKind::AmbiguousProviders,
                required_by,
                capability,
                qualifier,
                provisions,
            )),
        }
    }

    fn error<'value, V: ?Sized + 'value>(
        kind: CompositionErrorKind,
        required_by: ModuleId,
        capability: CapabilityId,
        qualifier: Option<QualifierId>,
        provisions: impl Iterator<Item = (ModuleId, &'value V)>,
    ) -> CompositionError {
        let mut candidates = provisions.map(|(module, _)| module).collect::<Vec<_>>();
        candidates.sort_unstable();
        CompositionError::new(kind, required_by, capability, qualifier, candidates)
    }
}
